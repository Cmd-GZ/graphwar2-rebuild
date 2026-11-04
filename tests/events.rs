//! 事件枚举与生命表的验收。

use gw2_rebuild::events::{Event, GameInfo, GAME_INFO_FIELD_COUNT};
use gw2_rebuild::life::{LifeState, Status};

#[test]
fn event_order_matches_the_rodata_table() {
    // 顺序来自 .rodata 里连续的变体名（docs/07 §2）
    assert_eq!(Event::TickRequest.wire_tag(), 0);
    assert_eq!(Event::FunctionPoints.wire_tag(), 31);
    assert_eq!(Event::FunctionsCalculated.wire_tag(), 32);
}

#[test]
fn event_variants_are_contiguous() {
    // 判别值必须连续：中间多一个变体就说明表格抄错了
    let tags: Vec<u16> = [
        Event::TickRequest, Event::TickReply, Event::Subscribe,
        Event::NewConnectionRequest, Event::NewConnection, Event::ConnectionRemoved,
        Event::ConnectedToServer, Event::NewBotRequest, Event::ChangeTeamRequest,
        Event::RemovePlayerRequest, Event::PlayerInfo, Event::EntityRemoved,
        Event::SoldierInfo, Event::AddSoldierRequest, Event::RemoveSoldierRequest,
        Event::AxisModeChangeRequest, Event::FunctionModeChangeRequest,
        Event::TurnModeChangeRequest, Event::TimeModeChangeRequest,
        Event::GameNameRequest, Event::TargetInfo, Event::RemoveTeam, Event::PosInfo,
        Event::LifeQueuedDeath, Event::GameNameInfo, Event::GameStateInfo,
        Event::FunctionFireRequest, Event::FunctionFire, Event::FunctionUpdateRequest,
        Event::FunctionInfo, Event::FunctionActive, Event::FunctionPoints,
        Event::FunctionsCalculated,
    ].iter().map(|e| e.wire_tag()).collect();
    for (i, t) in tags.iter().enumerate() {
        assert_eq!(*t as usize, i, "第 {i} 个变体的判别值不是 {i}");
    }
}

#[test]
fn game_info_field_count_is_pinned() {
    assert_eq!(GAME_INFO_FIELD_COUNT, 9);
    let g = GameInfo::default();
    // 九个字段都能默认构造
    assert_eq!(g.current_player_count, 0);
    assert!(!g.locked);
}

#[test]
fn life_state_two_tables() {
    let mut s = LifeState::default();
    assert_eq!(s.lens(), (0, 0));

    // mode != 1：只写第一张表
    s.set(7, Status::Alive, None);
    assert_eq!(s.status(7), Some(Status::Alive));
    assert_eq!(s.lens(), (1, 0));

    // mode == 1：同时写第二张表
    s.set(7, Status::Dead, Some(120));
    assert_eq!(s.status(7), Some(Status::Dead));
    assert_eq!(s.lens(), (1, 1));
    assert_eq!(s.take_queued_death(7), Some(120));
    assert_eq!(s.lens(), (1, 0));
}

#[test]
fn unknown_soldier_is_not_assumed() {
    let s = LifeState::default();
    assert_eq!(s.status(42), None); // 不默认存活，也不默认死亡
}
