//! `Office` 안쪽 단위 시험.

use super::*;

#[test]
fn 시각은_지금과_이전_더하기_하나_중_큰_것() {
    assert_eq!(next_stamp(UnixMillis(10), UnixMillis(3)), UnixMillis(10));
    assert_eq!(next_stamp(UnixMillis(3), UnixMillis(3)), UnixMillis(4));
    assert_eq!(next_stamp(UnixMillis(1), UnixMillis(5)), UnixMillis(6));
    assert_eq!(
        next_stamp(UnixMillis(0), UnixMillis(i64::MAX)),
        UnixMillis(i64::MAX)
    );
}

struct Fixed;
impl Clock for Fixed {
    fn now(&self) -> UnixMillis {
        UnixMillis(7)
    }
}

struct Counter(std::sync::atomic::AtomicU64);
#[async_trait::async_trait]
impl EventSink for Counter {
    async fn publish(&self, _kind: &str, _payload: Value) -> Result<u64, crate::error::SinkError> {
        Ok(self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
}

#[tokio::test]
async fn 끝난_응답은_정해진_개수만_남긴다() {
    let office = Office::new(
        TeamState::Operating,
        Arc::new(Counter(std::sync::atomic::AtomicU64::new(1))),
        Arc::new(Fixed),
    );
    let mut first = None;
    for n in 0..(FINISHED_KEEP + 5) {
        let r = office
            .ask(Room::Direct, Agent::Risk, "q", &format!("m-{n}"))
            .await
            .unwrap();
        first.get_or_insert(r.request_id.clone());
        let failed = office.fail(&r.request_id, " ").await.unwrap();
        assert_eq!(failed.error.as_deref(), Some(UNKNOWN_FAILURE));
    }
    let state = office.state();
    assert_eq!(state.finished.len(), FINISHED_KEEP);
    assert_eq!(state.responses.len(), FINISHED_KEEP);
    drop(state);
    assert!(office.response(&first.unwrap()).is_none());
    let pending = office.pending_responses();
    assert!(pending.is_empty());
}
