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

/// 对局状态（原版 `GraphwarGameState`）。
///
/// **变体表是观测到的**，不再是"抓包里见过 6 个"：真值把 8 个变体名做成一张
/// `[&str; 8]` 静态表放在 `.data.rel.ro`（x86 `0x9586d0`–`0x958740`，逐项 16 字节
/// `(ptr, len)`，`len` 分别为 10/5/19/18/20/16/10/12），由
/// `GraphwarGameState::deserialize` 交给 serde（`0x46f0c0`）；同一组名字又被
/// `GraphwarGameState::serialize`（`0x46f020`）用一张 8 项的跳表
/// （`.rodata 0x1c17c0`）取出来写 `serialize_str`。
///
/// 本枚举的**声明顺序 = 那张表的顺序**，所以判别值就是原版的判别值（[`GameState::index`]）。
/// 抓包里只出现过中间 6 个：`Connecting` / `Disconnected` 是连接期状态，对局报文里不发
/// （`docs/14` §11.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Connecting,
    Setup,
    WaitingForFunctions,
    CalculatingResults,
    SynchronizingResults,
    ExecutingResults,
    EndingGame,
    Disconnected,
}

impl GameState {
    /// 原版的 8 个变体，**按声明顺序**（= `.data.rel.ro` `0x9586d0` 那张表的顺序）。
    pub const VARIANTS: [GameState; 8] = [
        GameState::Connecting,
        GameState::Setup,
        GameState::WaitingForFunctions,
        GameState::CalculatingResults,
        GameState::SynchronizingResults,
        GameState::ExecutingResults,
        GameState::EndingGame,
        GameState::Disconnected,
    ];

    /// 原版的判别值（写进 `[self+0x120]` 的那个字节，`set_state` `0x4780cc`）。
    ///
    /// 依据：`serialize` 的跳表按判别值索引变体名表 ⇒ 判别值 = 声明序。
    /// `handle_tick`（`0x470989`）拿 `cmp BYTE [self+0x120], 2` 判"正在等开火"，
    /// 而它判的正是 `WaitingForFunctions` —— 与这里的 2 对得上（`docs/14` §11.3）。
    pub fn index(self) -> u8 {
        match self {
            GameState::Connecting => 0,
            GameState::Setup => 1,
            GameState::WaitingForFunctions => 2,
            GameState::CalculatingResults => 3,
            GameState::SynchronizingResults => 4,
            GameState::ExecutingResults => 5,
            GameState::EndingGame => 6,
            GameState::Disconnected => 7,
        }
    }

    /// 协议里用的名字。
    pub fn as_str(self) -> &'static str {
        match self {
            GameState::Connecting => "Connecting",
            GameState::Setup => "Setup",
            GameState::WaitingForFunctions => "WaitingForFunctions",
            GameState::CalculatingResults => "CalculatingResults",
            GameState::SynchronizingResults => "SynchronizingResults",
            GameState::ExecutingResults => "ExecutingResults",
            GameState::EndingGame => "EndingGame",
            GameState::Disconnected => "Disconnected",
        }
    }

    /// 从协议名解析。
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "Connecting" => GameState::Connecting,
            "Disconnected" => GameState::Disconnected,
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

/// 原版 `handle_tick`（`0x470840`）第 ② 段的判定：**这一 tick 要不要自动结算这一回合**。
///
/// 反汇编（`0x470989`–`0x4709ec`，见 `docs/14` §11.3）逐条对应：
///
/// ```text
/// 0x470989  cmp BYTE [self+0x120], 2      ; state == WaitingForFunctions
/// 0x470999  call TurnManager::get_end_of_turn_tick
/// 0x4709d7  cmp eax, 1                    ; 取到了截止 tick（`Some`）
/// 0x4709dc  cmp tick, end                 ; now_tick >= end_of_turn_tick
/// 0x4709e0  cmp BYTE [self+0x11c], 0      ; "有人在等"标志
/// 0x4709ec  call fire_functions           ; ⇒ 自动开火
/// ```
///
/// 这条就是 `docs/117` §6 记的"回合超时的**检测**没有证据"：检测点在这里，
/// 判据是**时钟**（`end_of_turn_tick`），不是"谁按了按钮"。本函数只做判定，
/// 不接线——`Match` 上没有 `[+0x11c]` 那一位（`docs/14` §11.3 的"未接线"）。
pub fn should_auto_fire(
    state: GameState,
    now_tick: i64,
    end_of_turn_tick: Option<i64>,
    waiting_for_functions: bool,
) -> bool {
    state == GameState::WaitingForFunctions
        && waiting_for_functions
        && matches!(end_of_turn_tick, Some(end) if now_tick >= end)
}

/// 原版 `set_state`（`0x4780c0`）在 `EndingGame` 分支里算出来的**胜方字节**
/// （写进 `[self+0x11f]`，`0x478236`）。
///
/// 反汇编 `0x478133`–`0x478236` 是四行表：
///
/// | `Team1` 有活体 | `Team2` 有活体 | 回合上限设过 | 返回值 |
/// |---|---|---|---|
/// | 否 | 否 | — | `2`（没人赢） |
/// | 否 | 是 | — | `1`（`Team2`） |
/// | 是 | 否 | — | `0`（`Team1`） |
/// | 是 | 是 | 否 | `2` |
/// | 是 | 是 | 是 | `1`（`Team2`） |
///
/// 第四/五行是 `cmp DWORD [self+0xc8], 1` + `adc al, 1` 的直译：`[+0xc8]` 是
/// `set_turn_limit`（`0x472405`）写进去的**第一个参数**，也就是"这局设过上限"标志，
/// **不是**"上限已经用尽"。两者只在"两边都还有人却进了 `EndingGame`"这一种收场上分叉，
/// 而那种收场只可能由上限触发，所以 [`crate::game::Match::ending`] 用 `turn_count >= limit`
/// 判出来的结果在可达输入上一致（`docs/14` §11.4）。
pub fn winner_byte(team1_alive: bool, team2_alive: bool, turn_limit_set: bool) -> u8 {
    match (team1_alive, team2_alive) {
        (false, false) => 2,
        (false, true) => 1,
        (true, false) => 0,
        (true, true) => {
            if turn_limit_set {
                1
            } else {
                2
            }
        }
    }
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
