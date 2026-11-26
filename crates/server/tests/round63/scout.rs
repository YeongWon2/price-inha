//! 깨우기 턴의 세션: 새 공시로 뉴스 공시 시세 담당(scout)이 깨어 턴 하나를 돌면, 그 턴이 연 `wake-…` 세션은
//! 턴이 끝난 뒤 닫힌다(끝낸 시각 · 이유가 남는다). 깨우기마다 제목이 달라 닫지 않으면 열린 세션이 끝없이 쌓인다.

use std::time::{Duration, Instant};

use pi_store::{AgentSession, Store};

use crate::waking::{Plan, WATCHED, kst, monday};

/// scout의 깨우기 세션들.
fn wake_sessions(store: &Store) -> Vec<AgentSession> {
    store
        .sessions_of("scout")
        .unwrap()
        .into_iter()
        .filter(|session| session.title.starts_with("wake-"))
        .collect()
}

// 검증: E30 N1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 깨우기_턴이_끝나면_그_세션은_닫힌다() {
    let bench = Plan::demo()
        .polling(Duration::from_secs(1))
        .start(kst(monday(), 10, 0))
        .await;
    bench.ready().await;
    bench.advance(40).await;
    bench.data.disclose(WATCHED, "20261116000963");

    let store = bench.store();
    let deadline = Instant::now() + Duration::from_secs(30);
    while wake_sessions(&store).is_empty() {
        assert!(Instant::now() < deadline, "새 공시로 scout가 깨어나야 한다");
        bench.advance(20).await;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let open: Vec<_> = wake_sessions(&store)
            .into_iter()
            .filter(|session| session.ended_at.is_none())
            .collect();
        if open.is_empty() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "깨우기 턴이 끝난 세션은 닫혀야 한다: {open:?}"
        );
        bench.advance(20).await;
    }
    for session in wake_sessions(&store) {
        assert_eq!(
            session.end_reason.as_deref(),
            Some("wake_done"),
            "닫은 까닭이 남는다: {session:?}"
        );
    }
    bench.close().await;
}
