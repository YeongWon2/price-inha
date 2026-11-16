//! 둘째 이벤트가 실패해도 이미 들어간 응답은 오류로 돌려주지 않는다(office 2차 후보 1).
//! 응답 이벤트는 나갔고 응답은 상태에 들어갔다. 담당 모습은 "마지막으로 낸 모습"이 그대로라
//! 다음 바꿈 때 다시 나간다.

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pi_office::{
    Agent, ChatStatus, Clock, EVENT_AGENT, EVENT_CHAT, EventSink, Office, Room, SinkError,
    TeamState, Work,
};
use pi_store::UnixMillis;
use serde_json::Value;

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> UnixMillis {
        UnixMillis(self.0.fetch_add(10, Ordering::SeqCst))
    }
}

/// 정한 순번(1부터)의 publish만 실패하는 포트. 성공한 이벤트를 적어 둔다.
struct FailAt {
    calls: AtomicUsize,
    fail_calls: Vec<usize>,
    done: Mutex<Vec<(String, Value)>>,
}

impl FailAt {
    fn new(fail_calls: &[usize]) -> Arc<FailAt> {
        Arc::new(FailAt {
            calls: AtomicUsize::new(0),
            fail_calls: fail_calls.to_vec(),
            done: Mutex::new(Vec::new()),
        })
    }
    fn kinds(&self) -> Vec<String> {
        self.done
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.0.clone())
            .collect()
    }
    fn last_agent_work(&self) -> Option<String> {
        let done = self.done.lock().unwrap();
        let (_, payload) = done.iter().rev().find(|e| e.0 == EVENT_AGENT)?;
        payload["work"].as_str().map(str::to_owned)
    }
}

#[async_trait]
impl EventSink for FailAt {
    async fn publish(&self, kind: &str, payload: Value) -> Result<u64, SinkError> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_calls.contains(&n) {
            return Err(SinkError {
                message: "저장 실패(시험)".to_owned(),
            });
        }
        let mut done = self.done.lock().unwrap();
        done.push((kind.to_owned(), payload));
        Ok(done.len() as u64)
    }
}

fn work_of(office: &Office, agent: Agent) -> Work {
    office
        .agents()
        .iter()
        .find(|v| v.agent == agent)
        .unwrap()
        .work
}

fn office_with(sink: &Arc<FailAt>) -> Office {
    let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
    Office::new(TeamState::Operating, sink.clone(), clock)
}

// 검증: E37 C5
#[tokio::test]
async fn 말하기에서_담당_모습_이벤트만_실패하면_응답은_돌려주고_모습은_다음에_다시_낸다() {
    // 호출 1: 응답 queued 이벤트, 호출 2: 담당 모습(대기) 이벤트 ← 실패
    let sink = FailAt::new(&[2]);
    let office = office_with(&sink);
    let response = office
        .ask(Room::Direct, Agent::Risk, "질문", "m-1")
        .await
        .expect("응답은 이미 들어갔으니 오류가 아니다");
    assert_eq!(response.status, ChatStatus::Queued);
    assert_eq!(sink.kinds(), [EVENT_CHAT]);
    assert_eq!(
        office.response(&response.request_id),
        Some(response.clone())
    );
    assert_eq!(office.pending_responses().len(), 1);

    // 담당 모습은 못 낸 그대로(마지막으로 낸 모습)다.
    assert_eq!(work_of(&office, Agent::Risk), Work::Idle);

    // 다음 바꿈(같은 팀 상태 다시 알림)에서 못 낸 모습이 나간다.
    let seqs = office.set_team_state(TeamState::Operating).await.unwrap();
    assert_eq!(seqs.len(), 1);
    assert_eq!(sink.last_agent_work().as_deref(), Some("queued"));
    assert_eq!(work_of(&office, Agent::Risk), Work::Queued);
}

// 검증: E37 C5
#[tokio::test]
async fn 생각_시작에서_담당_모습_이벤트만_실패해도_생각_중_응답을_돌려준다() {
    // 호출 1·2: ask(응답 · 모습), 3: thinking 응답, 4: 모습(생각 중) ← 실패
    let sink = FailAt::new(&[4]);
    let office = office_with(&sink);
    let asked = office
        .ask(Room::Direct, Agent::News, "질문", "m-1")
        .await
        .unwrap();
    let started = office
        .start_next(Agent::News)
        .await
        .expect("응답은 이미 thinking이다")
        .expect("대기 중인 응답이 있었다");
    assert_eq!(started.request_id, asked.request_id);
    assert_eq!(started.status, ChatStatus::Thinking);
    assert_eq!(
        office.response(&asked.request_id).unwrap().status,
        ChatStatus::Thinking
    );
    office.set_team_state(TeamState::Operating).await.unwrap();
    assert_eq!(sink.last_agent_work().as_deref(), Some("thinking"));
}

// 검증: E37 C5 E6
#[tokio::test]
async fn 끝내기에서_담당_모습_이벤트만_실패하면_끝난_응답을_돌려주고_다시_끝낼_수_없다() {
    // 1·2 ask, 3·4 start_next, 5 complete 응답, 6 모습(대기 끝) ← 실패
    let sink = FailAt::new(&[6]);
    let office = office_with(&sink);
    let asked = office
        .ask(Room::Direct, Agent::Review, "질문", "m-1")
        .await
        .unwrap();
    office.start_next(Agent::Review).await.unwrap().unwrap();
    let done = office
        .complete(&asked.request_id, "답")
        .await
        .expect("응답은 이미 끝났다");
    assert_eq!(done.status, ChatStatus::Complete);
    assert!(matches!(
        office.complete(&asked.request_id, "또").await,
        Err(pi_office::Error::Finished(_))
    ));
    assert!(office.pending_responses().is_empty());

    office.set_team_state(TeamState::Operating).await.unwrap();
    assert_eq!(sink.last_agent_work().as_deref(), Some("idle"));
}

// 검증: E37
#[tokio::test]
async fn 응답_이벤트가_실패하면_여전히_오류이고_응답은_들어가지_않는다() {
    let sink = FailAt::new(&[1]);
    let office = office_with(&sink);
    let err = office
        .ask(Room::Direct, Agent::Risk, "질문", "m-1")
        .await
        .unwrap_err();
    assert!(matches!(err, pi_office::Error::Publish(_)), "{err:?}");
    assert!(office.pending_responses().is_empty());
}
