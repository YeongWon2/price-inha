//! 공지 나눠 주기(6차, 결정 17 · C19 · E45 · E46 · N18).
//! 저장소는 임시 폴더에 열고, 시계 · 이벤트 포트 · 가리기 · AI 상한 판정은 테스트가 끼운다. 네트워크를 쓰지 않는다.

use std::collections::HashSet;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_office::{
    Agent, AiBudget, ChatStatus, Clock, EVENT_CHAT, EVENT_NOTICE, EVENT_NOTICE_ACK, Error,
    EventSink, NoticeTurn, Office, Place, Redactor, Room, SinkError, TeamState, Work,
};
use pi_store::{NoticeAckStatus, Store, UnixMillis};
use serde_json::{Value, json};
use tempfile::TempDir;

// ---------- 테스트가 끼우는 포트 ----------

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.load(Ordering::SeqCst))
    }
}

/// 받은 이벤트를 기억만 하는 포트. 번호는 100부터 붙인다.
struct Recorder {
    next: AtomicU64,
    events: Mutex<Vec<(String, Value)>>,
}

impl Recorder {
    fn of_kind(&self, kind: &str) -> Vec<Value> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k == kind)
            .map(|(_, p)| p.clone())
            .collect()
    }
    fn all_text(&self) -> String {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|(k, p)| format!("{k} {p}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
    fn len(&self) -> usize {
        self.events.lock().unwrap().len()
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

/// 시험용 비밀 표시 `TESTSECRET`을 `[가림]`으로 바꾼다.
struct TestRedactor;

impl Redactor for TestRedactor {
    fn redact(&self, text: &str) -> String {
        text.replace("TESTSECRET", "[가림]")
    }
}

/// 정한 담당만 상한으로 막는 판정.
struct DenyAgents(Vec<Agent>);

impl AiBudget for DenyAgents {
    fn try_take(&self, agent: Agent) -> bool {
        !self.0.contains(&agent)
    }
}

struct Setup {
    office: Office,
    rec: Arc<Recorder>,
    store: Store,
    _dir: TempDir,
}

fn setup_with(team: TeamState, budget: Option<Vec<Agent>>) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("pi.db")).unwrap();
    let rec = Arc::new(Recorder {
        next: AtomicU64::new(100),
        events: Mutex::new(Vec::new()),
    });
    let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
    let mut office =
        Office::new(team, rec.clone(), clock).with_notices(store.clone(), Arc::new(TestRedactor));
    if let Some(denied) = budget {
        office = office.with_ai_budget(Arc::new(DenyAgents(denied)));
    }
    Setup {
        office,
        rec,
        store,
        _dir: dir,
    }
}

fn setup() -> Setup {
    setup_with(TeamState::Operating, None)
}

fn ack_status(s: &Setup, notice: pi_store::NoticeId, agent: Agent) -> NoticeAckStatus {
    s.store
        .get_notice(notice)
        .unwrap()
        .acks
        .into_iter()
        .find(|a| a.role == agent.id())
        .unwrap()
        .status
}

// ---------- 공지 받기 ----------

// 검증: C19, N18
#[tokio::test]
async fn 공지는_저장소에_남고_담당_여섯이_기다리며_확인_턴이_여섯_생긴다() {
    let s = setup();
    let post = s
        .office
        .post_notice("내일은 반도체를 먼저 봐 주세요", "n-1")
        .await
        .unwrap();
    let id = post.notice.id;
    assert_eq!(post.notice.text, "내일은 반도체를 먼저 봐 주세요");
    assert_eq!(post.notice.sent_at, UnixMillis(1_000));
    // 저장소: 담당 여섯 모두 기다림(Agent::ALL 순서, 이름은 담당 id)
    let stored = s.store.get_notice(id).unwrap();
    assert_eq!(stored, post.notice);
    let roles: Vec<&str> = stored.acks.iter().map(|a| a.role.as_str()).collect();
    let want: Vec<&str> = Agent::ALL.iter().map(|a| a.id()).collect();
    assert_eq!(roles, want);
    assert!(
        stored
            .acks
            .iter()
            .all(|a| a.status == NoticeAckStatus::Waiting)
    );

    // 확인 턴 여섯: 담당마다 팀 방 응답 객체 하나(queued, 공지 번호가 붙음)
    assert_eq!(post.acks.len(), 6);
    for (turn, agent) in post.acks.iter().zip(Agent::ALL) {
        assert_eq!(turn.agent, agent);
        assert_eq!(turn.state, NoticeTurn::Queued);
        assert_eq!(turn.state.as_str(), "queued");
        let r = turn.response.as_ref().unwrap();
        assert_eq!(r.agent, agent);
        assert_eq!(r.room, Room::Team);
        assert_eq!(r.status, ChatStatus::Queued);
        assert_eq!(r.source_message_id, "n-1");
        assert_eq!(r.notice, Some(id));
    }
    assert_eq!(s.office.pending_responses().len(), 6);
    // 확인 답은 담당 대기열에 든다(N18)
    assert!(s.office.agents().iter().all(|v| v.work == Work::Queued));
    let chats = s.rec.of_kind(EVENT_CHAT);
    assert_eq!(chats.len(), 6);
    assert!(chats.iter().all(|c| c["notice"] == json!(id.0)));
}

// 검증: C19
#[tokio::test]
async fn 확인_답이_오면_그_담당만_답함으로_바뀌고_이벤트가_나간다() {
    let s = setup();
    let post = s
        .office
        .post_notice("오늘은 쉬어 갑니다", "n-1")
        .await
        .unwrap();
    let id = post.notice.id;
    let thinking = s.office.start_next(Agent::News).await.unwrap().unwrap();
    assert_eq!(thinking.notice, Some(id));
    let done = s
        .office
        .complete(&thinking.request_id, "알겠습니다")
        .await
        .unwrap();
    assert_eq!(done.status, ChatStatus::Complete);

    let stored = s.store.get_notice(id).unwrap();
    let news = stored.acks.iter().find(|a| a.role == "news").unwrap();
    assert_eq!(news.status, NoticeAckStatus::Answered);
    assert_eq!(news.answer.as_deref(), Some("알겠습니다"));
    assert_eq!(news.answered_at, Some(UnixMillis(1_000)));
    for agent in Agent::ALL.into_iter().filter(|a| *a != Agent::News) {
        assert_eq!(ack_status(&s, id, agent), NoticeAckStatus::Waiting);
    }
    assert_eq!(
        s.rec.of_kind(EVENT_NOTICE_ACK),
        [json!({"notice": id.0, "agent": "news", "state": "answered"})]
    );
}

// 검증: E45, N18, C19
#[tokio::test]
async fn 상한에_걸린_담당은_상한으로_못_함이고_턴을_받지_않는다() {
    let s = setup_with(
        TeamState::Operating,
        Some(vec![Agent::Trader, Agent::Leader]),
    );
    let post = s
        .office
        .post_notice("이번 주는 조심합시다", "n-1")
        .await
        .unwrap();
    let id = post.notice.id;
    let limited: Vec<Agent> = post
        .acks
        .iter()
        .filter(|t| t.state == NoticeTurn::Limit)
        .map(|t| t.agent)
        .collect();
    assert_eq!(limited, [Agent::Trader, Agent::Leader]);
    assert_eq!(NoticeTurn::Limit.as_str(), "limit");
    assert!(
        post.acks
            .iter()
            .all(|t| (t.state == NoticeTurn::Limit) == t.response.is_none())
    );
    assert_eq!(ack_status(&s, id, Agent::Trader), NoticeAckStatus::Capped);
    assert_eq!(ack_status(&s, id, Agent::Leader), NoticeAckStatus::Capped);
    assert_eq!(ack_status(&s, id, Agent::News), NoticeAckStatus::Waiting);

    let pending: HashSet<Agent> = s
        .office
        .pending_responses()
        .iter()
        .map(|r| r.agent)
        .collect();
    assert_eq!(pending.len(), 4);
    assert!(!pending.contains(&Agent::Trader) && !pending.contains(&Agent::Leader));
    assert_eq!(s.rec.of_kind(EVENT_CHAT).len(), 4);
    let acks = s.rec.of_kind(EVENT_NOTICE_ACK);
    assert_eq!(acks.len(), 2);
    assert!(acks.contains(&json!({"notice": id.0, "agent": "trader", "state": "limit"})));
    assert!(acks.contains(&json!({"notice": id.0, "agent": "leader", "state": "limit"})));
}

// 검증: C19, E6
#[tokio::test]
async fn 확인_답이_실패하면_실패로_한_번만_바뀐다() {
    let s = setup();
    let id = s
        .office
        .post_notice("점검 공지", "n-1")
        .await
        .unwrap()
        .notice
        .id;

    // 실패로 끝냄
    let risk = s.office.start_next(Agent::Risk).await.unwrap().unwrap();
    s.office.fail(&risk.request_id, "timeout").await.unwrap();
    assert_eq!(ack_status(&s, id, Agent::Risk), NoticeAckStatus::Failed);
    // 다시 끝내려 해도 바뀌지 않는다
    let again = s
        .office
        .complete(&risk.request_id, "늦은 답")
        .await
        .unwrap_err();
    assert!(matches!(again, Error::Finished(_)), "{again:?}");
    assert_eq!(ack_status(&s, id, Agent::Risk), NoticeAckStatus::Failed);

    // 빈 답도 실패다
    let review = s.office.start_next(Agent::Review).await.unwrap().unwrap();
    s.office.complete(&review.request_id, "  ").await.unwrap();
    assert_eq!(ack_status(&s, id, Agent::Review), NoticeAckStatus::Failed);

    let acks = s.rec.of_kind(EVENT_NOTICE_ACK);
    assert_eq!(
        acks,
        [
            json!({"notice": id.0, "agent": "risk", "state": "failed"}),
            json!({"notice": id.0, "agent": "review", "state": "failed"}),
        ]
    );
}

// 검증: C19
#[tokio::test]
async fn 공지_글은_한_자에서_오백_자까지만_받는다() {
    let s = setup();
    for bad in [String::new(), "가".repeat(501)] {
        let err = s.office.post_notice(&bad, "n-x").await.unwrap_err();
        assert!(matches!(err, Error::InvalidNotice(_)), "{err:?}");
    }
    // 거부되면 아무것도 남지 않는다
    assert!(s.store.notices_page(None, 10).unwrap().is_empty());
    assert_eq!(s.rec.len(), 0);
    assert!(s.office.pending_responses().is_empty());

    // 경계: 500자는 받는다
    let ok = s
        .office
        .post_notice(&"가".repeat(500), "n-1")
        .await
        .unwrap();
    assert_eq!(ok.notice.text.chars().count(), 500);
    assert_eq!(s.store.notices_page(None, 10).unwrap().len(), 1);
}

// 검증: C19
#[tokio::test]
async fn 공지_글은_가린_뒤_저장하고_이벤트로_낸다() {
    let s = setup();
    let post = s
        .office
        .post_notice("키 TESTSECRET 는 쓰지 마세요", "n-1")
        .await
        .unwrap();
    assert_eq!(post.notice.text, "키 [가림] 는 쓰지 마세요");
    assert_eq!(
        s.store.get_notice(post.notice.id).unwrap().text,
        "키 [가림] 는 쓰지 마세요"
    );
    assert!(!s.rec.all_text().contains("TESTSECRET"));
    assert_eq!(
        s.rec.of_kind(EVENT_NOTICE),
        [json!({"notice": post.notice.id.0, "text": "키 [가림] 는 쓰지 마세요", "at": 1_000})]
    );
}

// 검증: C19
#[tokio::test]
async fn 공지를_받으면_공지_이벤트가_하나_나간다() {
    assert_eq!(EVENT_NOTICE, "office.notice");
    assert_eq!(EVENT_NOTICE_ACK, "office.notice_ack");
    let s = setup();
    let post = s.office.post_notice("오후 회의 없음", "n-1").await.unwrap();
    let notices = s.rec.of_kind(EVENT_NOTICE);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0]["notice"], json!(post.notice.id.0));
    assert_eq!(notices[0]["text"], "오후 회의 없음");
    assert_eq!(notices[0]["at"], 1_000);
    // 아직 확인 상태는 바뀌지 않았다
    assert!(s.rec.of_kind(EVENT_NOTICE_ACK).is_empty());
}

// 검증: E46
#[tokio::test]
async fn 공지는_팀_상태와_담당_자리를_바꾸지_않는다() {
    let s = setup_with(TeamState::FullStop, None);
    s.office
        .post_notice("손절을 넓혀, 승인해, 다시 시작해", "n-1")
        .await
        .unwrap();
    // 멈춘 팀은 여전히 모두 침대(팀 상태가 바뀌지 않았다)
    assert!(s.office.agents().iter().all(|v| v.place == Place::Bed));
    assert!(
        s.rec
            .of_kind(pi_office::EVENT_AGENT)
            .iter()
            .all(|p| p["place"] == "bed")
    );
}

// 검증: C19
#[tokio::test]
async fn 공지_저장소가_없으면_공지를_받지_않는다() {
    let rec = Arc::new(Recorder {
        next: AtomicU64::new(100),
        events: Mutex::new(Vec::new()),
    });
    let office = Office::new(
        TeamState::Operating,
        rec.clone(),
        Arc::new(TestClock(AtomicI64::new(1_000))),
    );
    let err = office.post_notice("공지", "n-1").await.unwrap_err();
    assert!(matches!(err, Error::NoticeUnavailable), "{err:?}");
    assert_eq!(rec.len(), 0);
}

// 검증: C5, D20
#[tokio::test]
async fn 공지와_상관없는_응답의_이벤트에는_공지_칸이_없다() {
    let s = setup();
    let r = s
        .office
        .ask(Room::Direct, Agent::Risk, "손절선은요", "m-1")
        .await
        .unwrap();
    assert_eq!(r.notice, None);
    let chats = s.rec.of_kind(EVENT_CHAT);
    assert_eq!(chats.len(), 1);
    assert!(chats[0].get("notice").is_none());
}
