//! 팀 방 멘션 · 응답자 정하기 · 이어 말하기(6차, 결정 17 · C18 · E43 · E44 · N11).
//! 원본 DeskRPG 오피스 방과 같은 동작을 본다. 시계와 이벤트 포트는 테스트가 끼운다. 네트워크를 쓰지 않는다.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_office::{
    Agent, ChatStatus, Clock, EVENT_CHAT, Error, EventSink, Member, Office, RELAY_BUDGET, Relayed,
    Room, SinkError, TeamHint, TeamState, mention_tag, mentioned_names, parse_mentions,
};
use pi_store::UnixMillis;
use serde_json::Value;

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
        next: AtomicU64::new(100),
        events: Mutex::new(Vec::new()),
    });
    let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
    let office = Office::new(TeamState::Operating, rec.clone(), clock);
    (office, rec)
}

/// 시험용 명단(표시 이름은 시험이 정한다).
fn roster() -> Vec<Member> {
    [
        (Agent::News, "소식"),
        (Agent::Strategy, "전략"),
        (Agent::Risk, "위험"),
        (Agent::Review, "복기"),
        (Agent::Trader, "매매"),
        (Agent::Leader, "팀장"),
    ]
    .into_iter()
    .map(|(agent, name)| Member {
        agent,
        name: name.to_owned(),
    })
    .collect()
}

fn name_of(agent: Agent) -> String {
    roster()
        .into_iter()
        .find(|m| m.agent == agent)
        .unwrap()
        .name
}

/// 그 담당의 다음 응답을 생각 중으로 바꾸고 `content`로 끝낸다.
async fn answer(office: &Office, agent: Agent, content: &str) -> Relayed {
    let thinking = office.start_next(agent).await.unwrap().unwrap();
    office
        .complete_with_relay(&thinking.request_id, content, &roster())
        .await
        .unwrap()
}

/// `first`부터 `first` ↔ `second`가 번갈아 서로를 멘션하며 `n`번 이어 말한다. 매번 이어진 응답이 하나여야 한다.
/// 끝나면 다음에 답할 담당을 돌려준다.
async fn relay_back_and_forth(office: &Office, first: Agent, second: Agent, n: usize) -> Agent {
    let (mut speaker, mut other) = (first, second);
    for round in 0..n {
        let text = format!("{} 이어서 봐 주세요", mention_tag(&name_of(other)));
        let relayed = answer(office, speaker, &text).await;
        assert_eq!(relayed.follow_ups.len(), 1, "{round}번째 이어 말하기");
        assert_eq!(relayed.follow_ups[0].agent, other);
        (speaker, other) = (other, speaker);
    }
    speaker
}

// ---------- 멘션 해석(순수 함수) ----------

// 검증: C18
#[test]
fn 멘션은_표시_이름과_정확히_같을_때만_담당이_된다() {
    let r = roster();
    assert_eq!(parse_mentions("@[전략] 오늘 어때요", &r), [Agent::Strategy]);
    // 부분 일치 · 앞뒤 공백 · 괄호 없는 멘션은 담당이 아니다(원본: 정확 일치 + 괄호 형식)
    assert!(parse_mentions("@[전략가] 오늘 어때요", &r).is_empty());
    assert!(parse_mentions("@[전] 오늘 어때요", &r).is_empty());
    assert!(parse_mentions("@[ 전략] 오늘 어때요", &r).is_empty());
    assert!(parse_mentions("@전략 오늘 어때요", &r).is_empty());
    assert!(parse_mentions("멘션 없는 글", &r).is_empty());
}

// 검증: C18
#[test]
fn 같은_담당을_여러_번_불러도_한_번이고_글에_나온_순서다() {
    let r = roster();
    assert_eq!(
        parse_mentions("@[전략] @[위험] 그리고 다시 @[전략]", &r),
        [Agent::Strategy, Agent::Risk]
    );
    assert_eq!(
        parse_mentions("@[팀장] 먼저, 다음 @[소식] 그리고 @[매매]", &r),
        [Agent::Leader, Agent::News, Agent::Trader]
    );
}

// 검증: C18, E43
#[test]
fn 명단에_없는_이름은_무시한다() {
    let r = roster();
    assert_eq!(
        parse_mentions("@[사장] @[위험] 봐 주세요", &r),
        [Agent::Risk]
    );
    assert!(parse_mentions("@[사장] 봐 주세요", &r).is_empty());
    // 이름 목록은 명단과 상관없이 글에 나온 대로(같은 이름은 한 번)
    assert_eq!(
        mentioned_names("@[사장] @[위험] @[사장] @없음"),
        ["사장".to_owned(), "위험".to_owned()]
    );
    assert!(mentioned_names("@사장 괄호 없음").is_empty());
}

// 검증: C18
#[test]
fn 이름_속_괄호와_역슬래시는_가려서_쓰고_풀어서_읽는다() {
    let odd = "a]b\\c[";
    assert_eq!(mention_tag(odd), "@[a\\]b\\\\c\\[]");
    assert_eq!(mention_tag("전략"), "@[전략]");
    let r = vec![Member {
        agent: Agent::Review,
        name: odd.to_owned(),
    }];
    let text = format!("{} 확인", mention_tag(odd));
    assert_eq!(parse_mentions(&text, &r), [Agent::Review]);
    assert_eq!(mentioned_names(&text), [odd.to_owned()]);
}

// ---------- 응답자 정하기 ----------

// 검증: C18, C5
#[tokio::test]
async fn 팀_방_멘션_하나면_그_담당의_응답_하나가_생긴다() {
    let (office, rec) = office();
    let post = office
        .post_team("@[위험] 손절선 괜찮아요?", "m-1", &roster())
        .await
        .unwrap();
    assert_eq!(post.responders, [Agent::Risk]);
    assert_eq!(post.hint, None);
    assert_eq!(post.responses.len(), 1);
    let r = &post.responses[0];
    assert_eq!(r.agent, Agent::Risk);
    assert_eq!(r.room, Room::Team);
    assert_eq!(r.status, ChatStatus::Queued);
    assert_eq!(r.source_message_id, "m-1");
    assert_eq!(office.pending_responses(), post.responses);

    let events = rec.of_kind(EVENT_CHAT);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["requestId"], r.request_id.as_str());
    assert_eq!(events[0]["room"], "team");
    assert_eq!(events[0]["agent"], "risk");
    assert_eq!(events[0]["status"], "queued");
}

// 검증: C18, C5
#[tokio::test]
async fn 팀_방_멘션_둘이면_응답_둘이_멘션_순서로_생긴다() {
    let (office, rec) = office();
    let post = office
        .post_team("@[매매] @[소식] 둘 다 봐 주세요", "m-1", &roster())
        .await
        .unwrap();
    assert_eq!(post.responders, [Agent::Trader, Agent::News]);
    let agents: Vec<Agent> = post.responses.iter().map(|r| r.agent).collect();
    assert_eq!(agents, [Agent::Trader, Agent::News]);
    assert_ne!(post.responses[0].request_id, post.responses[1].request_id);
    assert!(post.responses.iter().all(|r| r.room == Room::Team
        && r.status == ChatStatus::Queued
        && r.source_message_id == "m-1"));
    assert_eq!(rec.of_kind(EVENT_CHAT).len(), 2);
    // 담당마다 자기 대기열에서 차례가 온다(지금의 대기열 규칙 그대로)
    let t = office.start_next(Agent::Trader).await.unwrap().unwrap();
    assert_eq!(t.request_id, post.responses[0].request_id);
    let n = office.start_next(Agent::News).await.unwrap().unwrap();
    assert_eq!(n.request_id, post.responses[1].request_id);
}

// 검증: C18, E44
#[tokio::test]
async fn 멘션이_없으면_아무도_답하지_않고_글은_받아들인다() {
    let (office, rec) = office();
    let post = office
        .post_team("다들 수고했어요", "m-1", &roster())
        .await
        .unwrap();
    assert!(post.responders.is_empty());
    assert!(post.responses.is_empty());
    assert_eq!(post.hint, Some(TeamHint::NoMention));
    assert_eq!(TeamHint::NoMention.as_str(), "no_mention");
    assert!(rec.of_kind(EVENT_CHAT).is_empty());
    assert!(office.pending_responses().is_empty());
}

// 검증: C18, E43
#[tokio::test]
async fn 멘션한_이름이_명단과_맞지_않으면_아무도_답하지_않는다() {
    let (office, rec) = office();
    let post = office
        .post_team("@[사장] @[전략가] 봐 주세요", "m-1", &roster())
        .await
        .unwrap();
    assert!(post.responders.is_empty());
    assert!(post.responses.is_empty());
    assert_eq!(post.hint, Some(TeamHint::NoMatch));
    assert_eq!(TeamHint::NoMatch.as_str(), "no_match");
    assert!(rec.of_kind(EVENT_CHAT).is_empty());
}

// 검증: C18
#[tokio::test]
async fn 빈_팀_방_글은_거부한다() {
    let (office, rec) = office();
    let err = office.post_team("   ", "m-1", &roster()).await.unwrap_err();
    assert!(matches!(err, Error::EmptyMessage), "{err:?}");
    assert!(rec.of_kind(EVENT_CHAT).is_empty());
}

// ---------- 이어 말하기 ----------

// 검증: C18, N11
#[tokio::test]
async fn 답_속_멘션으로_다른_담당이_이어서_답한다() {
    let (office, rec) = office();
    office
        .post_team("@[소식] 오늘 뉴스 정리해 주세요", "m-1", &roster())
        .await
        .unwrap();
    let relayed = answer(&office, Agent::News, "정리했습니다. @[전략] 의견 주세요").await;
    assert_eq!(relayed.response.status, ChatStatus::Complete);
    assert_eq!(
        relayed.response.content,
        "정리했습니다. @[전략] 의견 주세요"
    );
    assert_eq!(relayed.follow_ups.len(), 1);
    let f = &relayed.follow_ups[0];
    assert_eq!(f.agent, Agent::Strategy);
    assert_eq!(f.room, Room::Team);
    assert_eq!(f.status, ChatStatus::Queued);
    assert_eq!(f.source_message_id, "m-1");
    assert_eq!(office.pending_responses(), relayed.follow_ups);
    let events = rec.of_kind(EVENT_CHAT);
    assert!(
        events
            .iter()
            .any(|e| e["requestId"] == f.request_id.as_str() && e["status"] == "queued")
    );
}

// 검증: N11, C18
#[tokio::test]
async fn 이어_말하기는_한_사용자_글에서_여섯_번에서_멈춘다() {
    assert_eq!(RELAY_BUDGET, 6);
    let (office, _) = office();
    office
        .post_team("@[소식] 시작해 주세요", "m-1", &roster())
        .await
        .unwrap();
    let next = relay_back_and_forth(&office, Agent::News, Agent::Strategy, RELAY_BUDGET).await;
    // 일곱 번째 멘션은 이어지지 않는다
    let other = if next == Agent::News {
        Agent::Strategy
    } else {
        Agent::News
    };
    let text = format!("{} 한 번 더", mention_tag(&name_of(other)));
    let last = answer(&office, next, &text).await;
    assert_eq!(last.response.status, ChatStatus::Complete);
    assert!(last.follow_ups.is_empty());
    assert!(office.pending_responses().is_empty());
}

// 검증: N11
#[tokio::test]
async fn 자기_멘션은_무시하고_예산을_쓰지_않는다() {
    let (office, _) = office();
    // 자기만 멘션한 답은 이어지지 않는다
    office
        .post_team("@[소식] 혼자 정리해 주세요", "m-1", &roster())
        .await
        .unwrap();
    let alone = answer(&office, Agent::News, "@[소식] 제가 하겠습니다").await;
    assert!(alone.follow_ups.is_empty());

    // 자기와 상대를 함께 멘션해도 상대만 이어지고, 여섯 번을 다 쓸 수 있다(자기 멘션을 셌다면 세 번에서 멈춘다)
    office
        .post_team("@[소식] 다시 시작해 주세요", "m-2", &roster())
        .await
        .unwrap();
    let (mut speaker, mut other) = (Agent::News, Agent::Strategy);
    for round in 0..RELAY_BUDGET {
        let text = format!(
            "{} 정리했고 {} 이어 주세요",
            mention_tag(&name_of(speaker)),
            mention_tag(&name_of(other))
        );
        let relayed = answer(&office, speaker, &text).await;
        let agents: Vec<Agent> = relayed.follow_ups.iter().map(|r| r.agent).collect();
        assert_eq!(agents, [other], "{round}번째");
        (speaker, other) = (other, speaker);
    }
    let text = format!("{} 끝", mention_tag(&name_of(other)));
    assert!(answer(&office, speaker, &text).await.follow_ups.is_empty());
}

// 검증: N11
#[tokio::test]
async fn 아직_답하는_중인_담당은_다시_부르지_않고_예산도_쓰지_않는다() {
    let (office, _) = office();
    office
        .post_team("@[소식] @[전략] 둘 다 봐 주세요", "m-1", &roster())
        .await
        .unwrap();
    // 전략은 아직 대기 중이다: 다시 부르지 않는다
    let first = answer(&office, Agent::News, "@[전략] 덧붙여 주세요").await;
    assert!(first.follow_ups.is_empty());
    let pending = office.pending_responses();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].agent, Agent::Strategy);

    // 예산은 그대로 여섯이다: 전략부터 여섯 번 이어지고 일곱 번째는 멈춘다
    let next = relay_back_and_forth(&office, Agent::Strategy, Agent::News, RELAY_BUDGET).await;
    let other = if next == Agent::News {
        Agent::Strategy
    } else {
        Agent::News
    };
    let text = format!("{} 마지막", mention_tag(&name_of(other)));
    assert!(answer(&office, next, &text).await.follow_ups.is_empty());
}

// 검증: N11, C18
#[tokio::test]
async fn 한_답에서_같은_담당을_두_번_불러도_하나만_이어진다() {
    let (office, _) = office();
    office
        .post_team("@[소식] 정리해 주세요", "m-1", &roster())
        .await
        .unwrap();
    let relayed = answer(
        &office,
        Agent::News,
        "@[전략] @[전략] 그리고 @[위험] 봐 주세요",
    )
    .await;
    let agents: Vec<Agent> = relayed.follow_ups.iter().map(|r| r.agent).collect();
    assert_eq!(agents, [Agent::Strategy, Agent::Risk]);
    assert!(
        relayed
            .follow_ups
            .iter()
            .all(|r| r.source_message_id == "m-1")
    );
}

// 검증: N11
#[tokio::test]
async fn 사람이_다시_말하면_예산이_다시_찬다() {
    let (office, _) = office();
    office
        .post_team("@[소식] 시작", "m-1", &roster())
        .await
        .unwrap();
    let next = relay_back_and_forth(&office, Agent::News, Agent::Strategy, RELAY_BUDGET).await;
    let spent = answer(&office, next, "@[위험] 더").await;
    assert!(spent.follow_ups.is_empty());

    office
        .post_team("@[소식] 다시 봐 주세요", "m-2", &roster())
        .await
        .unwrap();
    let relayed = answer(&office, Agent::News, "@[위험] 봐 주세요").await;
    assert_eq!(relayed.follow_ups.len(), 1);
    assert_eq!(relayed.follow_ups[0].agent, Agent::Risk);
    assert_eq!(relayed.follow_ups[0].source_message_id, "m-2");
}

// 검증: C18, C5
#[tokio::test]
async fn 일대일_방의_답은_멘션이_있어도_이어지지_않는다() {
    let (office, _) = office();
    let asked = office
        .ask(Room::Direct, Agent::News, "정리해 주세요", "d-1")
        .await
        .unwrap();
    let relayed = answer(&office, Agent::News, "@[전략] 에게도 물어보세요").await;
    assert_eq!(relayed.response.request_id, asked.request_id);
    assert_eq!(relayed.response.room, Room::Direct);
    assert_eq!(relayed.response.status, ChatStatus::Complete);
    assert!(relayed.follow_ups.is_empty());
    assert!(office.pending_responses().is_empty());
}

// 검증: E6, N11
#[tokio::test]
async fn 빈_답은_실패이고_이어_말하지_않는다() {
    let (office, _) = office();
    office
        .post_team("@[소식] 정리해 주세요", "m-1", &roster())
        .await
        .unwrap();
    let relayed = answer(&office, Agent::News, "   ").await;
    assert_eq!(relayed.response.status, ChatStatus::Failed);
    assert_eq!(relayed.response.error.as_deref(), Some("empty_response"));
    assert!(relayed.follow_ups.is_empty());
}

// 검증: D20
#[tokio::test]
async fn 끝난_응답이나_없는_응답은_이어_말하기로_끝낼_수_없다() {
    let (office, _) = office();
    office
        .post_team("@[소식] 정리해 주세요", "m-1", &roster())
        .await
        .unwrap();
    let relayed = answer(&office, Agent::News, "끝").await;
    let again = office
        .complete_with_relay(&relayed.response.request_id, "@[전략] 다시", &roster())
        .await
        .unwrap_err();
    assert!(matches!(again, Error::Finished(_)), "{again:?}");
    let unknown = office
        .complete_with_relay("chat-없음", "@[전략]", &roster())
        .await
        .unwrap_err();
    assert!(matches!(unknown, Error::UnknownRequest(_)), "{unknown:?}");
    assert!(office.pending_responses().is_empty());
}
