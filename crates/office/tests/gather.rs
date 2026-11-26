//! 멘션하면 담당이 내 앞으로 모인다(결정 29 · D12): 팀 방 글을 받은 그때 멘션된 담당 모두가 곧바로 오는 중이 된다.
//! 답 턴을 기다리지 않는다. 이미 와 있거나 오는 중이면 다시 부르지 않는다. 논의 중이면 오지 않는다.
//! 시계와 이벤트 포트는 테스트가 끼운다. 잠깐도 자지 않는다. 네트워크를 쓰지 않는다.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_office::{
    Activity, Agent, Clock, EVENT_AGENT, Error, EventSink, Office, SinkError, SummonPhase,
    SummonReason, TeamState,
};
use pi_store::UnixMillis;
use serde_json::Value;

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

/// 받은 이벤트를 기억만 하는 포트.
struct Recorder {
    next: AtomicU64,
    events: Mutex<Vec<(String, Value)>>,
}

impl Recorder {
    fn summons_of(&self, agent: Agent) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, p)| k == EVENT_AGENT && p["agent"] == agent.id())
            .map(|(_, p)| p.clone())
            .collect()
    }
}

#[async_trait]
impl EventSink for Recorder {
    async fn publish(&self, kind: &str, payload: Value) -> Result<u64, SinkError> {
        let seq = self.next.fetch_add(1, Ordering::SeqCst);
        self.events.lock().unwrap().push((kind.to_owned(), payload));
        Ok(seq)
    }
}

fn office() -> (Office, Arc<Recorder>) {
    let rec = Arc::new(Recorder {
        next: AtomicU64::new(1),
        events: Mutex::new(Vec::new()),
    });
    let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
    (Office::new(TeamState::Operating, rec.clone(), clock), rec)
}

fn phase_of(office: &Office, agent: Agent) -> (SummonPhase, Option<SummonReason>) {
    let v = office
        .agents()
        .into_iter()
        .find(|v| v.agent == agent)
        .unwrap();
    (v.phase, v.reason)
}

// 검증: D12 M12 C20
#[tokio::test]
async fn 멘션된_담당_모두가_한꺼번에_오는_중이_된다() {
    let (office, rec) = office();
    let got = office
        .mention_posted(&[Agent::Risk, Agent::Strategy, Agent::News])
        .await;
    assert_eq!(got.len(), 3, "{got:?}");
    for (agent, result) in &got {
        assert_eq!(
            result.as_ref().ok(),
            Some(&SummonPhase::Called),
            "{agent:?}"
        );
    }
    for agent in [Agent::Risk, Agent::Strategy, Agent::News] {
        assert_eq!(
            phase_of(&office, agent),
            (SummonPhase::Called, Some(SummonReason::Mention))
        );
        let last = rec.summons_of(agent).pop().expect("모습 이벤트");
        assert_eq!(last["phase"], "called");
        assert_eq!(last["reason"], "mention");
    }
    for agent in [Agent::Review, Agent::Trader, Agent::Leader] {
        assert_eq!(phase_of(&office, agent).0, SummonPhase::Idle, "{agent:?}");
        assert!(
            rec.summons_of(agent).is_empty(),
            "멘션하지 않은 담당({agent:?})은 오지 않는다"
        );
    }
}

// 검증: D12 M12
#[tokio::test]
async fn 이미_와_있거나_오는_중인_담당은_다시_부르지_않는다() {
    let (office, rec) = office();
    office.summon(Agent::Risk).await.unwrap();
    office.summon_arrived(Agent::Risk).await.unwrap();
    office.mention_posted(&[Agent::Strategy]).await;
    let before_risk = rec.summons_of(Agent::Risk).len();
    let before_strategy = rec.summons_of(Agent::Strategy).len();

    let got = office
        .mention_posted(&[Agent::Risk, Agent::Strategy, Agent::Strategy])
        .await;
    assert_eq!(got.len(), 2, "같은 담당은 한 번만 본다: {got:?}");
    assert_eq!(
        phase_of(&office, Agent::Risk),
        (SummonPhase::Waiting, Some(SummonReason::Call)),
        "부름으로 곁에 와 있는 담당은 그대로다"
    );
    assert_eq!(phase_of(&office, Agent::Strategy).0, SummonPhase::Called);
    assert_eq!(rec.summons_of(Agent::Risk).len(), before_risk);
    assert_eq!(rec.summons_of(Agent::Strategy).len(), before_strategy);
}

// 검증: D12 E47
#[tokio::test]
async fn 논의_중인_담당만_빠지고_나머지는_온다() {
    let (office, _rec) = office();
    office
        .set_activity(Agent::Trader, Activity::Discussing)
        .await
        .unwrap();
    let got = office.mention_posted(&[Agent::Trader, Agent::Leader]).await;
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(
        matches!(
            got[0],
            (
                Agent::Trader,
                Err(Error::MeetingReserved {
                    agent: Agent::Trader
                })
            )
        ),
        "{got:?}"
    );
    assert!(
        matches!(got[1], (Agent::Leader, Ok(SummonPhase::Called))),
        "{got:?}"
    );
    assert_eq!(phase_of(&office, Agent::Trader).0, SummonPhase::Idle);
    assert_eq!(phase_of(&office, Agent::Leader).0, SummonPhase::Called);
}
