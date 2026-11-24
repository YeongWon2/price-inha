//! 담당 부르기 단계(결정 17 · D12)와 오래된 "일하는 중" 풀기(N15).
//! 시계와 이벤트 포트는 테스트가 끼운다. 시간은 테스트가 정하고 잠깐도 자지 않는다. 네트워크를 쓰지 않는다.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_office::{
    Activity, Agent, AgentView, ChatStatus, Clock, EVENT_AGENT, Error, EventSink, Office, Place,
    Room, SinkError, SummonPhase, SummonReason, TeamState, Work,
};
use pi_store::UnixMillis;
use serde_json::Value;

// ---------- 테스트가 끼우는 포트 ----------

struct TestClock(AtomicI64);

impl TestClock {
    fn at(ms: i64) -> Arc<TestClock> {
        Arc::new(TestClock(AtomicI64::new(ms)))
    }
    fn set(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

/// 화면 없이 받은 이벤트를 기억만 하는 포트. 번호는 100부터 붙인다.
struct Recorder {
    next: AtomicU64,
    events: Mutex<Vec<(u64, String, Value)>>,
}

impl Recorder {
    fn new() -> Arc<Recorder> {
        Arc::new(Recorder {
            next: AtomicU64::new(100),
            events: Mutex::new(Vec::new()),
        })
    }
    fn of_kind(&self, kind: &str) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, k, _)| k == kind)
            .map(|(_, _, p)| p.clone())
            .collect()
    }
    fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }
}

#[async_trait]
impl EventSink for Recorder {
    async fn publish(&self, kind: &str, payload: Value) -> Result<u64, SinkError> {
        let seq = self.next.fetch_add(1, Ordering::SeqCst);
        self.events
            .lock()
            .unwrap()
            .push((seq, kind.to_owned(), payload));
        Ok(seq)
    }
}

/// 시각 1_000에서 시작하는 운용 중 사무실.
fn office() -> (Office, Arc<Recorder>, Arc<TestClock>) {
    let rec = Recorder::new();
    let clock = TestClock::at(1_000);
    let office = Office::new(TeamState::Operating, rec.clone(), clock.clone());
    (office, rec, clock)
}

fn view(office: &Office, agent: Agent) -> AgentView {
    office
        .agents()
        .into_iter()
        .find(|v| v.agent == agent)
        .unwrap()
}

fn agent_events_for(rec: &Recorder, agent: Agent) -> Vec<Value> {
    rec.of_kind(EVENT_AGENT)
        .into_iter()
        .filter(|p| p["agent"] == agent.id())
        .collect()
}

/// 그 담당의 `office.agent` 이벤트들의 부르기 단계 이름(번호 순서).
fn phases_for(rec: &Recorder, agent: Agent) -> Vec<String> {
    agent_events_for(rec, agent)
        .iter()
        .map(|p| p["phase"].as_str().unwrap_or_default().to_owned())
        .collect()
}

// ---------- 부르기: 자리 → 오는 중 → 곁에서 기다림 → 돌아가는 중 → 자리 ----------

// 검증: D12 C20 M12
#[tokio::test]
async fn 부르면_오는_중이_되고_모습_이벤트에_단계가_실린다() {
    let (office, rec, _clock) = office();
    assert_eq!(view(&office, Agent::News).phase, SummonPhase::Idle);
    assert_eq!(view(&office, Agent::News).reason, None);
    let before = rec.len();

    let phase = office.summon(Agent::News).await.unwrap();
    assert_eq!(phase, SummonPhase::Called);

    let v = view(&office, Agent::News);
    assert_eq!(v.phase, SummonPhase::Called);
    assert_eq!(v.reason, Some(SummonReason::Call));
    assert_eq!(v.phase_at, UnixMillis(1_000));
    // 다른 담당은 그대로다.
    for other in Agent::ALL.into_iter().filter(|a| *a != Agent::News) {
        assert_eq!(view(&office, other).phase, SummonPhase::Idle);
    }

    // 이벤트는 하나. 옛 칸(agent · place · work)은 그대로 있고 새 칸(phase · reason · phaseAt)이 더해진다.
    assert_eq!(rec.len(), before + 1);
    let events = agent_events_for(&rec, Agent::News);
    let last = events.last().unwrap();
    assert_eq!(last["agent"], "news");
    assert_eq!(last["place"], "desk");
    assert_eq!(last["work"], "idle");
    assert_eq!(last["phase"], "called");
    assert_eq!(last["reason"], "call");
    assert_eq!(last["phaseAt"], 1_000);
}

// 검증: D12 C20 M12
#[tokio::test]
async fn 도착_알림에_곁에서_기다림이_된다() {
    let (office, rec, clock) = office();
    office.summon(Agent::Risk).await.unwrap();
    clock.set(4_000);

    let phase = office.summon_arrived(Agent::Risk).await.unwrap();
    assert_eq!(phase, SummonPhase::Waiting);
    let v = view(&office, Agent::Risk);
    assert_eq!(v.phase, SummonPhase::Waiting);
    assert_eq!(v.reason, Some(SummonReason::Call));
    assert_eq!(v.phase_at, UnixMillis(4_000));
    assert_eq!(phases_for(&rec, Agent::Risk), ["called", "waiting"]);
}

// 검증: D12 N10
#[tokio::test]
async fn 곁에서_열_초_조용하면_돌아가는_중이_되고_돌아옴_알림에_자리로_간다() {
    let (office, rec, clock) = office();
    office.summon(Agent::Strategy).await.unwrap();
    clock.set(2_000);
    office.summon_arrived(Agent::Strategy).await.unwrap();
    let events_before = rec.len();

    // 9.999초: 아직 기다린다.
    clock.set(2_000 + 9_999);
    let seqs = office.tick().await.unwrap();
    assert!(seqs.is_empty());
    assert_eq!(rec.len(), events_before);
    assert_eq!(view(&office, Agent::Strategy).phase, SummonPhase::Waiting);

    // 10초: 돌아간다.
    clock.set(2_000 + 10_000);
    let seqs = office.tick().await.unwrap();
    assert_eq!(seqs.len(), 1);
    let v = view(&office, Agent::Strategy);
    assert_eq!(v.phase, SummonPhase::Returning);
    assert_eq!(v.phase_at, UnixMillis(12_000));

    // 화면이 돌아왔다고 알리면 자리다. 까닭은 비운다.
    clock.set(15_000);
    let phase = office.summon_returned(Agent::Strategy).await.unwrap();
    assert_eq!(phase, SummonPhase::Idle);
    let v = view(&office, Agent::Strategy);
    assert_eq!(v.phase, SummonPhase::Idle);
    assert_eq!(v.reason, None);
    assert_eq!(
        phases_for(&rec, Agent::Strategy),
        ["called", "waiting", "returning", "idle"]
    );
    let last = agent_events_for(&rec, Agent::Strategy).pop().unwrap();
    assert_eq!(last["reason"], Value::Null);
}

// 검증: D12 N10
#[tokio::test]
async fn 곁에서_대화가_있으면_열_초가_다시_센다() {
    let (office, _rec, clock) = office();
    office.summon(Agent::Leader).await.unwrap();
    clock.set(2_000);
    office.summon_arrived(Agent::Leader).await.unwrap();

    // 5초 뒤 대화가 있었다.
    clock.set(7_000);
    office.note_conversation(Agent::Leader);

    // 도착에서 10초가 지났어도 마지막 대화에서는 아직이다.
    clock.set(12_000);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::Leader).phase, SummonPhase::Waiting);
    clock.set(7_000 + 9_999);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::Leader).phase, SummonPhase::Waiting);

    // 마지막 대화에서 10초.
    clock.set(7_000 + 10_000);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::Leader).phase, SummonPhase::Returning);
}

// ---------- 거절 · 다시 부르기 ----------

// 검증: D12 C20 E47
#[tokio::test]
async fn 회의_중인_담당은_부르기를_거절하고_회의를_끊지_않는다() {
    let (office, rec, _clock) = office();
    office
        .set_activity(Agent::Trader, Activity::Discussing)
        .await
        .unwrap();
    assert_eq!(view(&office, Agent::Trader).place, Place::Meeting);
    let before = rec.len();

    let err = office.summon(Agent::Trader).await.unwrap_err();
    assert!(
        matches!(
            err,
            Error::MeetingReserved {
                agent: Agent::Trader
            }
        ),
        "{err:?}"
    );

    let v = view(&office, Agent::Trader);
    assert_eq!(v.phase, SummonPhase::Idle);
    assert_eq!(v.place, Place::Meeting, "회의는 끊지 않는다");
    assert_eq!(rec.len(), before, "거절은 이벤트를 내지 않는다");
}

// 검증: D12 E47
#[tokio::test]
async fn 회의_중인_담당은_멘션으로도_걸어오지_않는다() {
    let (office, rec, _clock) = office();
    office
        .set_activity(Agent::Risk, Activity::Discussing)
        .await
        .unwrap();
    let before = rec.len();

    let err = office.mention_turn_started(Agent::Risk).await.unwrap_err();
    assert!(
        matches!(err, Error::MeetingReserved { agent: Agent::Risk }),
        "{err:?}"
    );
    assert_eq!(view(&office, Agent::Risk).phase, SummonPhase::Idle);
    assert_eq!(rec.len(), before);
}

// 검증: D12 C20
#[tokio::test]
async fn 이미_불려_온_담당을_다시_부르면_그대로다() {
    let (office, rec, clock) = office();
    office.summon(Agent::News).await.unwrap();
    let after_first = rec.len();

    // 오는 중에 다시 부름.
    clock.set(3_000);
    assert_eq!(
        office.summon(Agent::News).await.unwrap(),
        SummonPhase::Called
    );
    assert_eq!(rec.len(), after_first, "이벤트가 늘지 않는다");
    assert_eq!(view(&office, Agent::News).phase_at, UnixMillis(1_000));

    // 곁에서 기다리는 중에 다시 부름.
    office.summon_arrived(Agent::News).await.unwrap();
    let after_arrive = rec.len();
    clock.set(5_000);
    assert_eq!(
        office.summon(Agent::News).await.unwrap(),
        SummonPhase::Waiting
    );
    assert_eq!(rec.len(), after_arrive);
    assert_eq!(view(&office, Agent::News).phase_at, UnixMillis(3_000));
}

// 검증: D12 C20
#[tokio::test]
async fn 일하는_중인_담당도_부를_수_있다() {
    let (office, _rec, _clock) = office();
    let r = office
        .ask(Room::Direct, Agent::Review, "q", "m-1")
        .await
        .unwrap();
    office.start_next(Agent::Review).await.unwrap();
    assert_eq!(r.status, ChatStatus::Queued);
    assert_eq!(view(&office, Agent::Review).work, Work::Thinking);

    assert_eq!(
        office.summon(Agent::Review).await.unwrap(),
        SummonPhase::Called
    );
    let v = view(&office, Agent::Review);
    assert_eq!(v.phase, SummonPhase::Called);
    assert_eq!(v.work, Work::Thinking, "일 상태는 그대로다");
}

// ---------- 도착이 없을 때 ----------

// 검증: E48 N10 D12
#[tokio::test]
async fn 부른_뒤_삼십_초_도착이_없으면_돌아가는_중을_거쳐_자리로_둔다() {
    let (office, rec, clock) = office();
    office.summon(Agent::Strategy).await.unwrap();

    clock.set(1_000 + 29_999);
    assert!(office.tick().await.unwrap().is_empty());
    assert_eq!(view(&office, Agent::Strategy).phase, SummonPhase::Called);

    clock.set(1_000 + 30_000);
    let seqs = office.tick().await.unwrap();
    assert_eq!(seqs.len(), 2, "돌아가는 중 · 자리, 이벤트 둘");
    let v = view(&office, Agent::Strategy);
    assert_eq!(v.phase, SummonPhase::Idle);
    assert_eq!(v.reason, None);
    assert_eq!(
        phases_for(&rec, Agent::Strategy),
        ["called", "returning", "idle"]
    );

    // 뒤늦게 온 도착 알림은 자리의 담당을 움직이지 않는다.
    let before = rec.len();
    clock.set(40_000);
    assert_eq!(
        office.summon_arrived(Agent::Strategy).await.unwrap(),
        SummonPhase::Idle
    );
    assert_eq!(rec.len(), before);
}

// 검증: D12
#[tokio::test]
async fn 도착이_이미_온_담당은_삼십_초_규칙으로_돌아가지_않는다() {
    let (office, _rec, clock) = office();
    office.summon(Agent::Leader).await.unwrap();
    clock.set(1_500);
    office.summon_arrived(Agent::Leader).await.unwrap();
    // 대화가 계속 이어지는 한 곁에 남는다.
    for t in [9_000, 17_000, 25_000, 33_000, 41_000] {
        clock.set(t);
        office.note_conversation(Agent::Leader);
        office.tick().await.unwrap();
        assert_eq!(view(&office, Agent::Leader).phase, SummonPhase::Waiting);
    }
}

// ---------- 멘션으로 오기 ----------

// 검증: D12 M12
#[tokio::test]
async fn 멘션된_담당은_답_턴이_시작될_때_오는_중이_된다() {
    let (office, rec, _clock) = office();
    let phase = office.mention_turn_started(Agent::Risk).await.unwrap();
    assert_eq!(phase, SummonPhase::Called);
    let v = view(&office, Agent::Risk);
    assert_eq!(v.phase, SummonPhase::Called);
    assert_eq!(v.reason, Some(SummonReason::Mention));
    let last = agent_events_for(&rec, Agent::Risk).pop().unwrap();
    assert_eq!(last["phase"], "called");
    assert_eq!(last["reason"], "mention");
}

// 검증: D12 M12
#[tokio::test]
async fn 멘션으로_온_담당은_방을_보는_동안_곁에_남는다() {
    let (office, _rec, clock) = office();
    office.mention_turn_started(Agent::News).await.unwrap();
    clock.set(3_000);
    office.summon_arrived(Agent::News).await.unwrap();

    // 대화가 없어도 열 초 규칙으로는 돌아가지 않는다.
    clock.set(3_000 + 120_000);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::News).phase, SummonPhase::Waiting);
}

// 검증: D12 M12
#[tokio::test]
async fn 사용자가_팀_방을_떠나면_멘션으로_온_담당이_돌아가는_중이_된다() {
    let (office, rec, clock) = office();
    office.mention_turn_started(Agent::News).await.unwrap();
    office.summon_arrived(Agent::News).await.unwrap();
    office.mention_turn_started(Agent::Risk).await.unwrap(); // 아직 오는 중
    clock.set(9_000);

    let seqs = office.user_left_room(Room::Team).await.unwrap();
    assert_eq!(seqs.len(), 2);
    for agent in [Agent::News, Agent::Risk] {
        let v = view(&office, agent);
        assert_eq!(v.phase, SummonPhase::Returning, "{agent:?}");
        assert_eq!(v.phase_at, UnixMillis(9_000));
        assert_eq!(v.reason, Some(SummonReason::Mention));
    }
    assert_eq!(
        phases_for(&rec, Agent::News),
        ["called", "waiting", "returning"]
    );

    // 돌아옴 알림에 자리다.
    office.summon_returned(Agent::News).await.unwrap();
    assert_eq!(view(&office, Agent::News).phase, SummonPhase::Idle);
}

// 검증: D12
#[tokio::test]
async fn 방을_떠나도_부름으로_온_담당과_다른_방_떠남은_그대로다() {
    let (office, rec, _clock) = office();
    office.summon(Agent::Trader).await.unwrap();
    office.summon_arrived(Agent::Trader).await.unwrap();
    office.mention_turn_started(Agent::Leader).await.unwrap();
    let before = rec.len();

    // 부름으로 온 담당은 팀 방을 떠나도 그대로다.
    let seqs = office.user_left_room(Room::Team).await.unwrap();
    assert_eq!(seqs.len(), 1, "멘션으로 온 팀장만 돌아간다");
    assert_eq!(rec.len(), before + 1);
    assert_eq!(view(&office, Agent::Trader).phase, SummonPhase::Waiting);
    assert_eq!(view(&office, Agent::Leader).phase, SummonPhase::Returning);

    // 1:1 방을 떠난 것은 멘션으로 온 담당에게 상관없다.
    office.summon_returned(Agent::Leader).await.unwrap();
    office.mention_turn_started(Agent::Leader).await.unwrap();
    let before = rec.len();
    let seqs = office.user_left_room(Room::Direct).await.unwrap();
    assert!(seqs.is_empty());
    assert_eq!(rec.len(), before);
    assert_eq!(view(&office, Agent::Leader).phase, SummonPhase::Called);
}

// ---------- 오래된 "일하는 중" 풀기 ----------

// 검증: N15 E55
#[tokio::test]
async fn 대기열의_일이_삼백_초_넘게_그대로면_한가로_풀리고_이벤트_하나를_낸다() {
    let (office, rec, clock) = office();
    office
        .ask(Room::Direct, Agent::News, "q", "m-1")
        .await
        .unwrap();
    assert_eq!(view(&office, Agent::News).work, Work::Queued);
    let before = agent_events_for(&rec, Agent::News).len();

    // 299.999초: 아직 일하는 중.
    clock.set(1_000 + 299_999);
    assert!(office.tick().await.unwrap().is_empty());
    assert_eq!(view(&office, Agent::News).work, Work::Queued);

    // 300초: 한가로 풀린다.
    clock.set(1_000 + 300_000);
    let seqs = office.tick().await.unwrap();
    assert_eq!(seqs.len(), 1);
    assert_eq!(view(&office, Agent::News).work, Work::Idle);
    let events = agent_events_for(&rec, Agent::News);
    assert_eq!(events.len(), before + 1);
    assert_eq!(events.last().unwrap()["work"], "idle");

    // 풀기는 한 번만 한다.
    clock.set(1_000 + 900_000);
    assert!(office.tick().await.unwrap().is_empty());
    assert_eq!(agent_events_for(&rec, Agent::News).len(), before + 1);
}

// 검증: N15 E55
#[tokio::test]
async fn 생각_중인_일도_삼백_초_넘게_그대로면_풀린다() {
    let (office, rec, clock) = office();
    office
        .ask(Room::Direct, Agent::Strategy, "q", "m-1")
        .await
        .unwrap();
    office.start_next(Agent::Strategy).await.unwrap();
    assert_eq!(view(&office, Agent::Strategy).work, Work::Thinking);
    let before = agent_events_for(&rec, Agent::Strategy).len();

    clock.set(1_000 + 300_000);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::Strategy).work, Work::Idle);
    assert_eq!(agent_events_for(&rec, Agent::Strategy).len(), before + 1);
}

// 검증: N15
#[tokio::test]
async fn 일이_바뀌면_삼백_초를_바뀐_때부터_다시_센다() {
    let (office, _rec, clock) = office();
    office
        .ask(Room::Direct, Agent::Risk, "q", "m-1")
        .await
        .unwrap();
    // 200초 뒤 생각을 시작했다.
    clock.set(201_000);
    office.start_next(Agent::Risk).await.unwrap();

    // 처음 일에서는 300초가 넘었지만 마지막 바뀜에서는 아니다.
    clock.set(201_000 + 299_999);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::Risk).work, Work::Thinking);

    clock.set(201_000 + 300_000);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::Risk).work, Work::Idle);
}

// 검증: N15
#[tokio::test]
async fn 한가한_담당과_실패한_담당은_풀지_않고_이벤트도_내지_않는다() {
    let (office, rec, clock) = office();
    let r = office
        .ask(Room::Direct, Agent::Review, "q", "m-1")
        .await
        .unwrap();
    office.fail(&r.request_id, "boom").await.unwrap();
    assert_eq!(view(&office, Agent::Review).work, Work::Failed);
    let before = rec.len();

    clock.set(1_000 + 1_000_000);
    assert!(office.tick().await.unwrap().is_empty());
    assert_eq!(view(&office, Agent::Review).work, Work::Failed);
    assert_eq!(view(&office, Agent::News).work, Work::Idle);
    assert_eq!(rec.len(), before);
}

// 검증: N15 E55
#[tokio::test]
async fn 풀린_뒤_늦게_온_옛_상태는_버린다() {
    let (office, rec, clock) = office();
    office
        .ask(Room::Direct, Agent::News, "q", "m-1")
        .await
        .unwrap();
    clock.set(301_000);
    office.tick().await.unwrap();
    assert_eq!(view(&office, Agent::News).work, Work::Idle);
    let before = rec.len();

    // 풀리기 전 시각의 소식 · 풀린 시각과 같은 소식은 버린다.
    let dropped = office
        .report_work(Agent::News, Work::Thinking, UnixMillis(250_000))
        .await
        .unwrap();
    assert_eq!(dropped, None);
    let dropped = office
        .report_work(Agent::News, Work::Thinking, UnixMillis(301_000))
        .await
        .unwrap();
    assert_eq!(dropped, None);
    assert_eq!(view(&office, Agent::News).work, Work::Idle);
    assert_eq!(rec.len(), before, "버린 소식은 이벤트를 내지 않는다");

    // 더 새로운 소식은 받아들인다.
    clock.set(302_000);
    let seq = office
        .report_work(Agent::News, Work::Thinking, UnixMillis(301_500))
        .await
        .unwrap();
    assert!(seq.is_some());
    assert_eq!(view(&office, Agent::News).work, Work::Thinking);
    assert_eq!(rec.len(), before + 1);
}

// ---------- 메모리에만 ----------

// 검증: D12 M12
#[tokio::test]
async fn 다시_만들면_모두_자리에_있다() {
    let (office, _rec, _clock) = office();
    office.summon(Agent::News).await.unwrap();
    office.summon_arrived(Agent::News).await.unwrap();
    office.mention_turn_started(Agent::Risk).await.unwrap();
    assert_eq!(view(&office, Agent::News).phase, SummonPhase::Waiting);

    let rec = Recorder::new();
    let again = Office::new(TeamState::Operating, rec.clone(), TestClock::at(9_000));
    for v in again.agents() {
        assert_eq!(v.phase, SummonPhase::Idle, "{:?}", v.agent);
        assert_eq!(v.reason, None);
    }
    assert_eq!(rec.len(), 0, "만들 때는 이벤트를 내지 않는다");
}

// 검증: D12
#[tokio::test]
async fn 때가_안_된_시계는_아무것도_바꾸지_않는다() {
    let (office, rec, clock) = office();
    clock.set(500_000);
    assert!(office.tick().await.unwrap().is_empty());
    assert_eq!(rec.len(), 0);
    for v in office.agents() {
        assert_eq!(v.phase, SummonPhase::Idle);
        assert_eq!(v.work, Work::Idle);
    }
}
