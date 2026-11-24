//! 사무실 서버: 담당 모습 이벤트(`office.agent`)와 채팅 수명주기(`office.chat`).
//! 시계와 이벤트 포트는 테스트가 끼운다. 네트워크를 쓰지 않는다.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_office::{
    Activity, Agent, AgentView, CHAT_QUEUE_MAX, ChatResponse, ChatStatus, Clock, EVENT_AGENT,
    EVENT_CHAT, Error, EventSink, Office, Place, Room, SinkError, TeamState, Work,
};
use pi_store::{Store, UnixMillis};
use serde_json::{Value, json};

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
#[derive(Default)]
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
    fn all(&self) -> Vec<(u64, String, Value)> {
        self.events.lock().unwrap().clone()
    }
    fn of_kind(&self, kind: &str) -> Vec<Value> {
        self.all()
            .into_iter()
            .filter(|(_, k, _)| k == kind)
            .map(|(_, _, p)| p)
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

/// 실제 저장소(임시 폴더)에 쌓는 포트. gate의 Publisher가 하는 "저장하고 번호를 돌려줌"과 같다.
struct StoreSink(Mutex<Store>);

#[async_trait]
impl EventSink for StoreSink {
    async fn publish(&self, kind: &str, payload: Value) -> Result<u64, SinkError> {
        self.0
            .lock()
            .unwrap()
            .append(kind, &payload)
            .map_err(|e| SinkError {
                message: e.to_string(),
            })
    }
}

struct FailingSink;

#[async_trait]
impl EventSink for FailingSink {
    async fn publish(&self, _kind: &str, _payload: Value) -> Result<u64, SinkError> {
        Err(SinkError {
            message: "저장 실패(시험)".to_owned(),
        })
    }
}

fn office(team: TeamState) -> (Office, Arc<Recorder>, Arc<TestClock>) {
    let rec = Recorder::new();
    let clock = TestClock::at(1_000);
    let office = Office::new(team, rec.clone(), clock.clone());
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

fn chat_events_for(rec: &Recorder, request_id: &str) -> Vec<Value> {
    rec.of_kind(EVENT_CHAT)
        .into_iter()
        .filter(|p| p["requestId"] == request_id)
        .collect()
}

fn assert_send_sync<T: Send + Sync>() {}

// ---------- 담당 모습 ----------

// 검증: V1, V15, E37
#[tokio::test]
async fn 처음_모습은_팀_상태를_따르고_이벤트를_내지_않는다() {
    assert_send_sync::<Office>();
    let (resting, rec, _) = office(TeamState::Resting);
    let views = resting.agents();
    assert_eq!(
        views.iter().map(|v| v.agent).collect::<Vec<_>>(),
        Agent::ALL.to_vec()
    );
    for v in &views {
        assert_eq!(v.place, Place::Bed, "쉬는 중에는 침대: {:?}", v.agent);
        assert_eq!(v.work, Work::Idle);
    }
    assert_eq!(rec.len(), 0, "만들 때는 이벤트를 내지 않는다");

    let (operating, _, _) = office(TeamState::Operating);
    for v in operating.agents() {
        assert_eq!(v.place, Place::Desk, "운용 중에는 책상: {:?}", v.agent);
    }
}

// 검증: V13, V15, C2, E37
#[tokio::test]
async fn 출근하면_여섯_명이_책상으로_가는_번호_있는_이벤트를_낸다() {
    let (office, rec, _) = office(TeamState::Resting);

    let seqs = office.set_team_state(TeamState::Operating).await.unwrap();
    assert_eq!(seqs.len(), 6);
    let all = rec.all();
    assert_eq!(all.len(), 6);
    assert_eq!(
        all.iter().map(|(s, _, _)| *s).collect::<Vec<_>>(),
        seqs,
        "돌려준 번호는 포트가 붙인 번호다"
    );
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "번호가 커진다");
    let mut ids: Vec<String> = Vec::new();
    for (_, kind, payload) in &all {
        assert_eq!(kind, EVENT_AGENT);
        assert_eq!(payload["place"], "desk");
        assert_eq!(payload["work"], "idle");
        ids.push(payload["agent"].as_str().unwrap().to_owned());
    }
    ids.sort();
    let mut want: Vec<String> = Agent::ALL.iter().map(|a| a.id().to_owned()).collect();
    want.sort();
    assert_eq!(ids, want, "6명 모두 한 번씩");
    for v in office.agents() {
        assert_eq!(v.place, Place::Desk);
    }

    // 바뀌지 않으면 내지 않는다
    assert!(
        office
            .set_team_state(TeamState::Operating)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(rec.len(), 6);

    // 퇴근: 하루 한도 멈춤이면 모두 침대로
    let seqs = office
        .set_team_state(TeamState::DailyLimitHalt)
        .await
        .unwrap();
    assert_eq!(seqs.len(), 6);
    for (_, _, payload) in rec.all().iter().skip(6) {
        assert_eq!(payload["place"], "bed");
    }
    // 멈춤에서 쉬는 중으로: 자리는 그대로 침대라 이벤트 없음
    assert!(
        office
            .set_team_state(TeamState::Resting)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(rec.len(), 12);
}

// 검증: V13
#[tokio::test]
async fn 논의에_들어가면_탁자로_끝나면_책상으로_간다() {
    let (office, rec, _) = office(TeamState::Operating);

    let seq = office
        .set_activity(Agent::Risk, Activity::Discussing)
        .await
        .unwrap();
    assert!(seq.is_some());
    assert_eq!(view(&office, Agent::Risk).place, Place::Meeting);
    assert_eq!(
        view(&office, Agent::News).place,
        Place::Desk,
        "다른 담당은 그대로"
    );
    let events = rec.all();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, seq.unwrap());
    // (5차) 이벤트에 부르기 단계 칸이 더해져도 옛 세 칸은 그대로다.
    assert_eq!(events[0].2["agent"], json!("risk"));
    assert_eq!(events[0].2["place"], json!("meeting"));
    assert_eq!(events[0].2["work"], json!("idle"));

    // 같은 활동을 다시 알려도 이벤트 없음
    assert_eq!(
        office
            .set_activity(Agent::Risk, Activity::Discussing)
            .await
            .unwrap(),
        None
    );
    // 운용 중 일하기는 책상 그대로라 이벤트 없음
    assert_eq!(
        office
            .set_activity(Agent::News, Activity::Working)
            .await
            .unwrap(),
        None
    );
    assert_eq!(rec.len(), 1);

    // 논의 끝: 자기 책상으로 돌아간다
    assert!(
        office
            .set_activity(Agent::Risk, Activity::Idle)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(view(&office, Agent::Risk).place, Place::Desk);
    assert_eq!(
        agent_events_for(&rec, Agent::Risk).last().unwrap()["place"],
        "desk"
    );
}

// 검증: V13, V15
#[tokio::test]
async fn 논의_중에_팀이_멈추면_침대로_가고_다시_운용하면_탁자로_간다() {
    let (office, rec, _) = office(TeamState::Operating);
    office
        .set_activity(Agent::Leader, Activity::Discussing)
        .await
        .unwrap();
    office.set_team_state(TeamState::AnomalyHalt).await.unwrap();
    for v in office.agents() {
        assert_eq!(v.place, Place::Bed, "{:?}", v.agent);
    }
    assert_eq!(
        agent_events_for(&rec, Agent::Leader).last().unwrap()["place"],
        "bed"
    );
    office.set_team_state(TeamState::Operating).await.unwrap();
    assert_eq!(view(&office, Agent::Leader).place, Place::Meeting);
    assert_eq!(view(&office, Agent::Trader).place, Place::Desk);
}

// ---------- 채팅 수명주기 ----------

// 검증: C5, V14, D20
#[tokio::test]
async fn 담당에게_물으면_queued_thinking_complete로_답한다() {
    let (office, rec, _) = office(TeamState::Operating);

    let queued = office
        .ask(Room::Direct, Agent::Risk, "왜 샀어?", "m-1")
        .await
        .unwrap();
    assert_eq!(queued.status, ChatStatus::Queued);
    assert_eq!(queued.agent, Agent::Risk);
    assert_eq!(queued.room, Room::Direct);
    assert_eq!(queued.source_message_id, "m-1");
    assert_eq!(queued.content, "");
    assert_eq!(queued.error, None);
    assert!(!queued.request_id.is_empty());
    assert_eq!(view(&office, Agent::Risk).work, Work::Queued);

    let thinking = office.start_next(Agent::Risk).await.unwrap().unwrap();
    assert_eq!(thinking.request_id, queued.request_id);
    assert_eq!(thinking.status, ChatStatus::Thinking);
    assert_eq!(view(&office, Agent::Risk).work, Work::Thinking);

    let done = office
        .complete(&queued.request_id, "손절선이 가까워서 줄였습니다")
        .await
        .unwrap();
    assert_eq!(done.status, ChatStatus::Complete);
    assert_eq!(done.content, "손절선이 가까워서 줄였습니다");
    assert_eq!(done.source_message_id, "m-1");
    assert_eq!(done.error, None);
    assert_eq!(office.response(&queued.request_id), Some(done.clone()));
    assert_eq!(view(&office, Agent::Risk).work, Work::Idle);
    assert_eq!(view(&office, Agent::Risk).place, Place::Desk);

    // 바뀔 때마다 객체 전체를 번호 있는 이벤트로 낸다
    let events = chat_events_for(&rec, &queued.request_id);
    let statuses: Vec<&str> = events
        .iter()
        .map(|p| p["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["queued", "thinking", "complete"]);
    assert_eq!(
        events[2],
        json!({
            "requestId": done.request_id,
            "sourceMessageId": "m-1",
            "agent": "risk",
            "room": "direct",
            "status": "complete",
            "content": "손절선이 가까워서 줄였습니다",
            "updatedAt": done.updated_at.0,
            "error": null,
        })
    );
    // 담당 모습도 대기 → 생각 → 쉼으로 흐른다
    let works: Vec<String> = agent_events_for(&rec, Agent::Risk)
        .iter()
        .map(|p| p["work"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(works, ["queued", "thinking", "idle"]);
}

// 검증: C5, V14
#[tokio::test]
async fn 팀_방에서도_담당이_답한다() {
    let (office, rec, _) = office(TeamState::Operating);
    let r = office
        .ask(Room::Team, Agent::Leader, "오늘 어때?", "t-1")
        .await
        .unwrap();
    assert_eq!(r.room, Room::Team);
    assert_eq!(r.agent, Agent::Leader);
    office.start_next(Agent::Leader).await.unwrap().unwrap();
    office.complete(&r.request_id, "조용합니다").await.unwrap();
    let events = chat_events_for(&rec, &r.request_id);
    assert_eq!(events.len(), 3);
    for p in &events {
        assert_eq!(p["room"], "team");
        assert_eq!(p["agent"], "leader");
    }
}

// 검증: C5
#[tokio::test]
async fn 빈_글은_받지_않는다() {
    let (office, rec, _) = office(TeamState::Operating);
    for text in ["", "   ", "\n\t"] {
        let err = office
            .ask(Room::Direct, Agent::News, text, "m-x")
            .await
            .unwrap_err();
        assert!(matches!(err, Error::EmptyMessage), "{text:?}: {err:?}");
    }
    assert_eq!(rec.len(), 0);
    assert!(office.pending_responses().is_empty());
}

// 검증: D20
#[tokio::test]
async fn updated_at은_시계가_멈추거나_거꾸로_가도_커진다() {
    let (office, rec, clock) = office(TeamState::Operating);
    let a = office
        .ask(Room::Direct, Agent::Trader, "얼마에 샀어?", "m-1")
        .await
        .unwrap();
    // 시계가 그대로
    let b = office.start_next(Agent::Trader).await.unwrap().unwrap();
    assert!(b.updated_at > a.updated_at, "{a:?} → {b:?}");
    // 시계가 거꾸로
    clock.set(500);
    let c = office.complete(&a.request_id, "7만 원").await.unwrap();
    assert!(c.updated_at > b.updated_at, "{b:?} → {c:?}");
    // 이벤트의 updatedAt도 같은 순서
    let stamps: Vec<i64> = chat_events_for(&rec, &a.request_id)
        .iter()
        .map(|p| p["updatedAt"].as_i64().unwrap())
        .collect();
    assert_eq!(stamps, [a.updated_at.0, b.updated_at.0, c.updated_at.0]);

    // 시계가 앞서 있으면 그 시각을 쓴다(뒤처지지 않는다)
    clock.set(9_000);
    let d = office
        .ask(Room::Direct, Agent::Trader, "다음은?", "m-2")
        .await
        .unwrap();
    assert!(d.updated_at.0 >= 9_000, "{d:?}");
    assert_ne!(d.request_id, a.request_id, "응답 번호는 모두 다르다");
}

// 검증: D20
#[tokio::test]
async fn 끝난_응답은_바뀌지_않는다() {
    let (office, rec, _) = office(TeamState::Operating);
    let r = office
        .ask(Room::Direct, Agent::Review, "어제 매매는?", "m-1")
        .await
        .unwrap();
    office.start_next(Agent::Review).await.unwrap().unwrap();
    let done = office.complete(&r.request_id, "좋았습니다").await.unwrap();
    let before = rec.len();

    let err = office.complete(&r.request_id, "다른 답").await.unwrap_err();
    assert!(
        matches!(err, Error::Finished(ref id) if *id == r.request_id),
        "{err:?}"
    );
    let err = office.fail(&r.request_id, "timeout").await.unwrap_err();
    assert!(matches!(err, Error::Finished(_)), "{err:?}");
    assert_eq!(office.response(&r.request_id), Some(done));
    assert_eq!(rec.len(), before, "끝난 뒤에는 이벤트도 없다");

    // 실패로 끝난 것도 같다
    let f = office
        .ask(Room::Direct, Agent::Review, "또?", "m-2")
        .await
        .unwrap();
    let failed = office.fail(&f.request_id, "timeout").await.unwrap();
    let before = rec.len();
    assert!(matches!(
        office.complete(&f.request_id, "늦은 답").await.unwrap_err(),
        Error::Finished(_)
    ));
    assert_eq!(office.response(&f.request_id), Some(failed));
    assert_eq!(rec.len(), before);
}

// 검증: D20
#[tokio::test]
async fn 생각을_시작하지_않은_응답은_끝낼_수_없고_없는_번호는_거절한다() {
    let (office, rec, _) = office(TeamState::Operating);
    let r = office
        .ask(Room::Direct, Agent::Strategy, "전략은?", "m-1")
        .await
        .unwrap();
    let before = rec.len();
    let err = office.complete(&r.request_id, "답").await.unwrap_err();
    assert!(matches!(err, Error::InvalidTransition(_)), "{err:?}");
    assert_eq!(
        office.response(&r.request_id).unwrap().status,
        ChatStatus::Queued
    );
    assert_eq!(rec.len(), before);

    for err in [
        office.complete("없는-번호", "답").await.unwrap_err(),
        office.fail("없는-번호", "x").await.unwrap_err(),
    ] {
        assert!(matches!(err, Error::UnknownRequest(_)), "{err:?}");
    }
    assert_eq!(office.response("없는-번호"), None);
}

// 검증: E6, D20
#[tokio::test]
async fn 빈_답은_empty_response로_실패한다() {
    let (office, _, _) = office(TeamState::Operating);
    let r = office
        .ask(Room::Direct, Agent::News, "공시 있어?", "m-1")
        .await
        .unwrap();
    office.start_next(Agent::News).await.unwrap().unwrap();
    let done = office.complete(&r.request_id, "  \n ").await.unwrap();
    assert_eq!(done.status, ChatStatus::Failed);
    assert_eq!(done.error.as_deref(), Some("empty_response"));
    assert_eq!(view(&office, Agent::News).work, Work::Failed);
}

// 검증: E6, V14
#[tokio::test]
async fn 담당이_답하지_못하면_실패가_보인다() {
    let (office, rec, _) = office(TeamState::Operating);
    let r = office
        .ask(Room::Direct, Agent::Risk, "위험해?", "m-1")
        .await
        .unwrap();
    office.start_next(Agent::Risk).await.unwrap().unwrap();
    let failed = office.fail(&r.request_id, "timeout").await.unwrap();
    assert_eq!(failed.status, ChatStatus::Failed);
    assert_eq!(failed.error.as_deref(), Some("timeout"));
    assert_eq!(failed.content, "");
    assert_eq!(view(&office, Agent::Risk).work, Work::Failed);
    let last_chat = chat_events_for(&rec, &r.request_id).pop().unwrap();
    assert_eq!(last_chat["status"], "failed");
    assert_eq!(last_chat["error"], "timeout");
    assert_eq!(
        agent_events_for(&rec, Agent::Risk).last().unwrap()["work"],
        "failed"
    );
    assert_eq!(
        view(&office, Agent::News).work,
        Work::Idle,
        "다른 담당은 그대로"
    );

    // 대기 중에도 실패로 끝낼 수 있다
    let q = office
        .ask(Room::Team, Agent::Risk, "다시", "m-2")
        .await
        .unwrap();
    let qf = office.fail(&q.request_id, "adapter_error").await.unwrap();
    assert_eq!(qf.status, ChatStatus::Failed);

    // 다음 답이 나오면 실패 표시가 걷힌다
    let ok = office
        .ask(Room::Direct, Agent::Risk, "지금은?", "m-3")
        .await
        .unwrap();
    office.start_next(Agent::Risk).await.unwrap().unwrap();
    office.complete(&ok.request_id, "괜찮습니다").await.unwrap();
    assert_eq!(view(&office, Agent::Risk).work, Work::Idle);
}

// 검증: C5
#[tokio::test]
async fn 담당마다_먼저_온_순서로_하나씩_답한다() {
    let (office, _, _) = office(TeamState::Operating);
    let mut ids = Vec::new();
    for n in 1..=3 {
        let r = office
            .ask(
                Room::Direct,
                Agent::Risk,
                &format!("질문 {n}"),
                &format!("m-{n}"),
            )
            .await
            .unwrap();
        ids.push(r.request_id);
    }
    let news = office
        .ask(Room::Direct, Agent::News, "뉴스?", "n-1")
        .await
        .unwrap();

    let first = office.start_next(Agent::Risk).await.unwrap().unwrap();
    assert_eq!(first.request_id, ids[0]);
    assert_eq!(
        office.start_next(Agent::Risk).await.unwrap(),
        None,
        "생각 중인 것이 있으면 다음 것을 시작하지 않는다"
    );
    // 다른 담당은 따로 돈다
    let n = office.start_next(Agent::News).await.unwrap().unwrap();
    assert_eq!(n.request_id, news.request_id);
    assert_eq!(office.start_next(Agent::Strategy).await.unwrap(), None);

    office.complete(&ids[0], "하나").await.unwrap();
    let second = office.start_next(Agent::Risk).await.unwrap().unwrap();
    assert_eq!(second.request_id, ids[1]);
    office.fail(&ids[1], "timeout").await.unwrap();
    let third = office.start_next(Agent::Risk).await.unwrap().unwrap();
    assert_eq!(third.request_id, ids[2]);
    assert_eq!(third.source_message_id, "m-3");
}

// 검증: C5
#[tokio::test]
async fn 담당의_대기열은_8개까지다() {
    assert_eq!(CHAT_QUEUE_MAX, 8);
    let (office, rec, _) = office(TeamState::Operating);
    let mut first = None;
    for n in 0..CHAT_QUEUE_MAX {
        let r = office
            .ask(Room::Direct, Agent::Risk, "질문", &format!("m-{n}"))
            .await
            .unwrap();
        first.get_or_insert(r.request_id);
    }
    let chat_events = rec.of_kind(EVENT_CHAT).len();
    assert_eq!(chat_events, CHAT_QUEUE_MAX);

    let err = office
        .ask(Room::Direct, Agent::Risk, "아홉째", "m-9")
        .await
        .unwrap_err();
    assert!(
        matches!(err, Error::QueueFull { agent: Agent::Risk }),
        "{err:?}"
    );
    // 팀 방에서 같은 담당을 불러도 그 담당의 대기열 하나를 쓴다
    let err = office
        .ask(Room::Team, Agent::Risk, "팀 방에서", "t-9")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::QueueFull { .. }), "{err:?}");
    assert_eq!(
        rec.of_kind(EVENT_CHAT).len(),
        chat_events,
        "거절은 이벤트를 내지 않는다"
    );
    assert_eq!(office.pending_responses().len(), CHAT_QUEUE_MAX);

    // 다른 담당은 상관없다
    office
        .ask(Room::Direct, Agent::News, "뉴스?", "n-1")
        .await
        .unwrap();

    // 하나가 끝나면 다시 받는다
    let first = first.unwrap();
    office.start_next(Agent::Risk).await.unwrap().unwrap();
    office.complete(&first, "답").await.unwrap();
    office
        .ask(Room::Direct, Agent::Risk, "이제 됨", "m-10")
        .await
        .unwrap();
}

// 검증: E3, C2
#[tokio::test]
async fn 끝나지_않은_응답만_스냅샷에_있다() {
    let (office, _, _) = office(TeamState::Operating);
    let a = office
        .ask(Room::Direct, Agent::Risk, "하나", "m-1")
        .await
        .unwrap();
    let b = office
        .ask(Room::Team, Agent::News, "둘", "m-2")
        .await
        .unwrap();
    office.start_next(Agent::Risk).await.unwrap().unwrap();
    let mut pending: Vec<String> = office
        .pending_responses()
        .into_iter()
        .map(|r| r.request_id)
        .collect();
    pending.sort();
    let mut want = vec![a.request_id.clone(), b.request_id.clone()];
    want.sort();
    assert_eq!(pending, want);

    office.complete(&a.request_id, "끝").await.unwrap();
    let pending: Vec<ChatResponse> = office.pending_responses();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].request_id, b.request_id);
    assert_eq!(pending[0].status, ChatStatus::Queued);
}

// ---------- 화면 없이 쌓이고 다시 잇기 ----------

// 검증: E37, E3, C2, V15
#[tokio::test]
async fn 화면이_없어도_이벤트가_쌓이고_다시_이으면_놓친_것부터_받는다() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.db");
    let sink = Arc::new(StoreSink(Mutex::new(Store::open(&path).unwrap())));
    let office = Office::new(TeamState::Resting, sink.clone(), TestClock::at(1_000));

    // 화면이 하나도 없는 채로 운용이 돈다
    let seqs = office.set_team_state(TeamState::Operating).await.unwrap();
    assert_eq!(seqs.len(), 6);
    let last_seen = seqs[2]; // 화면은 셋째 이벤트까지 받고 끊겼다
    let r = office
        .ask(Room::Direct, Agent::Trader, "샀어?", "m-1")
        .await
        .unwrap();
    office.start_next(Agent::Trader).await.unwrap().unwrap();
    office.complete(&r.request_id, "샀습니다").await.unwrap();

    // 다시 이으면 last_seq 뒤의 것을 번호 순서로 빠짐없이 받는다
    let missed = sink.0.lock().unwrap().read_after(last_seen, 1_000).unwrap();
    let missed_seqs: Vec<u64> = missed.iter().map(|e| e.seq).collect();
    let want: Vec<u64> = (last_seen + 1..=last_seen + missed.len() as u64).collect();
    assert_eq!(missed_seqs, want, "번호가 빠짐없이 이어진다");
    let agent_desks = missed
        .iter()
        .filter(|e| e.kind == EVENT_AGENT && e.payload["place"] == "desk")
        .count();
    assert!(agent_desks >= 3, "놓친 출근 이벤트 3개가 있다");
    let chat: Vec<&str> = missed
        .iter()
        .filter(|e| e.kind == EVENT_CHAT)
        .map(|e| e.payload["status"].as_str().unwrap())
        .collect();
    assert_eq!(chat, ["queued", "thinking", "complete"]);

    // 다시 열어도 쌓인 것은 남아 있다
    let total = sink.0.lock().unwrap().read_after(0, 1_000).unwrap().len();
    drop(office);
    drop(sink);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.read_after(0, 1_000).unwrap().len(), total);
}

// 검증: E37
#[tokio::test]
async fn 포트가_실패하면_오류로_알린다() {
    let office = Office::new(
        TeamState::Resting,
        Arc::new(FailingSink),
        TestClock::at(1_000),
    );
    let err = office
        .set_team_state(TeamState::Operating)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Publish(_)), "{err:?}");
    let err = office
        .ask(Room::Direct, Agent::Risk, "질문", "m-1")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Publish(_)), "{err:?}");
}
