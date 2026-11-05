//! 回合状态机（观测自真实对局的 `GameStateInfo` 序列）。
//!
//! 抓包得到的顺序（每一回合都相同）：
//!
//! ```text
//! WaitingForFunctions
//!   --(玩家开火)-->          CalculatingResults
//!   --(算出结果)-->          SynchronizingResults
//!   --(到激活 tick)-->       ExecutingResults
//!   --(回合结束)-->          WaitingForFunctions
//! ```
//!
//! 全局还有 `Setup`（开局）与 `EndingGame`（分出胜负后）。
//!
//! 观测依据（一局 3 回合的完整序列，含一次 `EndingGame`）：
//!
//! ```text
//! Setup → WaitingForFunctions → CalculatingResults → SynchronizingResults
//!       → ExecutingResults → WaitingForFunctions → … → EndingGame → Setup
//! ```
//!
//! **未确定**：各状态持续的 tick 数（`EndOfTurnInfo.end_of_turn_tick` 的取值
//! 与我们的 60Hz tick 时钟对不上，见 `docs/41`）。

/// 对局状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Setup,
    WaitingForFunctions,
    CalculatingResults,
    SynchronizingResults,
    ExecutingResults,
    EndingGame,
}

impl GameState {
    /// 协议里用的名字。
    pub fn as_str(self) -> &'static str {
        match self {
            GameState::Setup => "Setup",
            GameState::WaitingForFunctions => "WaitingForFunctions",
            GameState::CalculatingResults => "CalculatingResults",
            GameState::SynchronizingResults => "SynchronizingResults",
            GameState::ExecutingResults => "ExecutingResults",
            GameState::EndingGame => "EndingGame",
        }
    }

    /// 从协议名解析。
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Setup" => GameState::Setup,
            "WaitingForFunctions" => GameState::WaitingForFunctions,
            "CalculatingResults" => GameState::CalculatingResults,
            "SynchronizingResults" => GameState::SynchronizingResults,
            "ExecutingResults" => GameState::ExecutingResults,
            "EndingGame" => GameState::EndingGame,
            _ => return None,
        })
    }
}

/// 驱动状态机的事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEvent {
    /// 玩家开火
    Fire,
    /// 结算完成（服务端算出结果）
    ResultsComputed,
    /// 到达激活 tick
    Activate,
    /// 回合结束
    TurnEnded,
    /// 分出胜负
    GameEnded,
    /// 开局（`Setup` -> 第一回合）
    GameStarted,
    /// 回到房间重开
    Reset,
}

/// 一次状态迁移。返回 `None` 表示该事件在当前状态下无意义（忽略）。
pub fn next_state(state: GameState, event: TurnEvent) -> Option<GameState> {
    use GameState::*;
    use TurnEvent::*;
    Some(match (state, event) {
        (Setup, GameStarted) => WaitingForFunctions,
        (WaitingForFunctions, Fire) => CalculatingResults,
        (CalculatingResults, ResultsComputed) => SynchronizingResults,
        (SynchronizingResults, Activate) => ExecutingResults,
        (ExecutingResults, TurnEnded) => WaitingForFunctions,
        (ExecutingResults, GameEnded) => EndingGame,
        (EndingGame, Reset) => Setup,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实对局里观察到的状态序列（相邻重复已去掉）。
    const CAPTURED: [GameState; 14] = [
        GameState::Setup,
        GameState::WaitingForFunctions,
        GameState::CalculatingResults,
        GameState::SynchronizingResults,
        GameState::ExecutingResults,
        GameState::WaitingForFunctions,
        GameState::CalculatingResults,
        GameState::SynchronizingResults,
        GameState::ExecutingResults,
        GameState::WaitingForFunctions,
        GameState::CalculatingResults,
        GameState::SynchronizingResults,
        GameState::ExecutingResults,
        GameState::EndingGame,
    ];

    /// 与上面状态序列一一对应的事件（从抓包的事件流推出，共 13 次迁移）。
    const EVENTS: [TurnEvent; 13] = [
        TurnEvent::GameStarted,
        TurnEvent::Fire,
        TurnEvent::ResultsComputed,
        TurnEvent::Activate,
        TurnEvent::TurnEnded,
        TurnEvent::Fire,
        TurnEvent::ResultsComputed,
        TurnEvent::Activate,
        TurnEvent::TurnEnded,
        TurnEvent::Fire,
        TurnEvent::ResultsComputed,
        TurnEvent::Activate,
        TurnEvent::GameEnded,
    ];

    #[test]
    fn reproduces_captured_state_sequence() {
        let mut state = CAPTURED[0];
        for (i, ev) in EVENTS.iter().enumerate() {
            state = next_state(state, *ev)
                .unwrap_or_else(|| panic!("第 {i} 个事件 {ev:?} 在 {state:?} 下无迁移"));
            let expect = CAPTURED[i + 1];
            assert_eq!(state, expect, "第 {i} 步：得到 {state:?}，抓包是 {expect:?}");
        }
    }

    #[test]
    fn state_names_round_trip() {
        for s in CAPTURED {
            assert_eq!(GameState::from_str(s.as_str()), Some(s));
        }
        assert_eq!(GameState::from_str("Nope"), None);
    }

    #[test]
    fn meaningless_events_are_ignored() {
        assert_eq!(next_state(GameState::WaitingForFunctions, TurnEvent::Activate), None);
        assert_eq!(next_state(GameState::Setup, TurnEvent::Reset), None);
    }
}
