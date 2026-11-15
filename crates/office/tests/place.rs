//! 있어야 할 곳의 투영: 순수 함수 표 시험(지시서 "테스트 주의").

use pi_office::{Activity, Agent, ChatStatus, Place, TeamState, Work, place_of};

// 검증: V1
#[test]
fn 담당은_여섯이고_이름이_정해져_있다() {
    let ids: Vec<&str> = Agent::ALL.iter().map(|a| a.id()).collect();
    assert_eq!(
        ids,
        ["news", "strategy", "risk", "review", "trader", "leader"]
    );
    for agent in Agent::ALL {
        assert_eq!(Agent::from_id(agent.id()), Some(agent));
    }
    assert_eq!(Agent::from_id("ceo"), None);
    assert_eq!(Agent::from_id(""), None);
    assert_eq!(Agent::from_id("News"), None, "이름은 대소문자를 가린다");
}

// 검증: V13, V15
#[test]
fn 팀_상태와_활동으로_자리를_정한다() {
    use Activity::{Discussing, Idle, Working};
    use Place::{Bed, Desk, Meeting};
    use TeamState::{AnomalyHalt, DailyLimitHalt, FullStop, Operating, Resting};
    let table = [
        // 운용 중: 출근해서 자기 책상, 논의하면 논의 탁자 (V13)
        (Operating, Idle, Desk),
        (Operating, Working, Desk),
        (Operating, Discussing, Meeting),
        // 쉬는 중 · 멈춤 · 정지: 활동과 상관없이 침대 (V15, 결정 25번: 팀 상태에 묶음)
        (Resting, Idle, Bed),
        (Resting, Working, Bed),
        (Resting, Discussing, Bed),
        (DailyLimitHalt, Idle, Bed),
        (DailyLimitHalt, Working, Bed),
        (DailyLimitHalt, Discussing, Bed),
        (AnomalyHalt, Idle, Bed),
        (AnomalyHalt, Working, Bed),
        (AnomalyHalt, Discussing, Bed),
        (FullStop, Idle, Bed),
        (FullStop, Working, Bed),
        (FullStop, Discussing, Bed),
    ];
    assert_eq!(
        table.len(),
        TeamState::ALL.len() * 3,
        "표가 모든 경우를 덮는다"
    );
    for (team, activity, want) in table {
        assert_eq!(place_of(team, activity), want, "{team:?} + {activity:?}");
    }
}

// 검증: V13, V15
#[test]
fn 자리와_상태의_이벤트_이름() {
    assert_eq!(Place::Bed.as_str(), "bed");
    assert_eq!(Place::Desk.as_str(), "desk");
    assert_eq!(Place::Meeting.as_str(), "meeting");
    assert_eq!(Work::Idle.as_str(), "idle");
    assert_eq!(Work::Queued.as_str(), "queued");
    assert_eq!(Work::Thinking.as_str(), "thinking");
    assert_eq!(Work::Failed.as_str(), "failed");
}

// 검증: C5, D20
#[test]
fn 응답_단계의_이름과_끝() {
    let table = [
        (ChatStatus::Queued, "queued", false),
        (ChatStatus::Thinking, "thinking", false),
        (ChatStatus::Complete, "complete", true),
        (ChatStatus::Failed, "failed", true),
    ];
    for (status, name, terminal) in table {
        assert_eq!(status.as_str(), name);
        assert_eq!(status.is_terminal(), terminal, "{status:?}");
    }
}
