//! 33차 8번: 새 경로의 관문은 사무실의 다른 쓰기와 같다(세션 · 원격 401 · 같은 출처) · 바깥 가짜 서버로 간 요청 0건.

use pi_agent::Role;
use price_inha::{
    DemoTurn, KIND_OFFICE_SUMMON, OFFICE_CHAT_PATH, OFFICE_NOTICE_PATH, OFFICE_NOTICES_PATH,
    OFFICE_SUMMON_ARRIVED_PATH, OFFICE_SUMMON_PATH, OFFICE_SUMMON_VIEW_PATH,
};
use serde_json::{Value, json};

use crate::common::{
    Bench, events, get, notice_body, origin_of, post_raw, session, team_text, wait_for,
};

/// 새 쓰기 경로와 그 몸통.
fn writes() -> Vec<(&'static str, Value)> {
    vec![
        (OFFICE_NOTICE_PATH, notice_body("몰래 공지", "g-1")),
        (OFFICE_SUMMON_PATH, json!({"agent": "risk"})),
        (OFFICE_SUMMON_ARRIVED_PATH, json!({"agent": "risk"})),
        (OFFICE_SUMMON_VIEW_PATH, json!({"room": "direct"})),
        (OFFICE_CHAT_PATH, team_text("@[리스크] 몰래", "g-2")),
    ]
}

/// 아무것도 남지 않았다: 공지 · 부르기 · 응답 이벤트가 없다.
fn nothing_left(bench: &Bench) {
    let store = bench.store();
    assert!(events(&store, pi_office::EVENT_NOTICE).is_empty());
    assert!(events(&store, KIND_OFFICE_SUMMON).is_empty());
    assert!(events(&store, pi_office::EVENT_CHAT).is_empty());
}

// 검증: C18 C19 C20 D2 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 세션이_없으면_새_경로가_모두_401이다() {
    let bench = Bench::start().await;
    let origin = origin_of(bench.addr());
    assert_eq!(
        get(bench.addr(), OFFICE_NOTICES_PATH, None).await.status,
        401
    );
    for (path, body) in writes() {
        let reply = post_raw(bench.addr(), path, None, Some(&origin), &body.to_string()).await;
        assert_eq!(reply.status, 401, "{path}: {}", reply.text);
    }
    nothing_left(&bench);
    bench.close().await;
}

// 검증: C18 C19 C20 D2 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 원격에서_세션이_없으면_401이다() {
    let bench = Bench::start().await;
    let origin = origin_of(bench.addr());
    bench.go_remote();
    assert_eq!(
        get(bench.addr(), OFFICE_NOTICES_PATH, None).await.status,
        401
    );
    for (path, body) in writes() {
        let reply = post_raw(bench.addr(), path, None, Some(&origin), &body.to_string()).await;
        assert_eq!(reply.status, 401, "{path}: {}", reply.text);
    }
    nothing_left(&bench);
    bench.close().await;
}

// 검증: C18 C19 C20 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_출처나_출처_없는_쓰기는_거부하고_아무것도_남기지_않는다() {
    let bench = Bench::start().await;
    let cookie = session(bench.addr()).await;
    // 포트가 다르면 다른 출처다(로컬 주소만 쓴다).
    let other = "http://127.0.0.1:9";
    for (path, body) in writes() {
        for origin in [Some(other), None] {
            let reply =
                post_raw(bench.addr(), path, Some(&cookie), origin, &body.to_string()).await;
            assert!(
                (400..500).contains(&reply.status),
                "{path} 출처 {origin:?}: {}",
                reply.status
            );
        }
    }
    nothing_left(&bench);
    let list = get(bench.addr(), OFFICE_NOTICES_PATH, Some(&cookie)).await;
    assert_eq!(list.status, 200, "{}", list.text);
    assert_eq!(list.body, json!([]), "거부한 공지는 남지 않는다");
    bench.close().await;
}

// 검증: C18 C19 C20 V58 V71 V60
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 멘션_공지_부르기를_해도_바깥_가짜_서버로_간_요청은_없다() {
    let bench = Bench::start().await;
    bench.script(DemoTurn::Chat(Role::Risk), "리스크입니다.");
    let reply = bench
        .post(OFFICE_CHAT_PATH, &team_text("@[리스크] 안녕", "o-1"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.pass(3);
    let reply = bench
        .post(OFFICE_NOTICE_PATH, &notice_body("바깥 요청 시험", "o-2"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let reply = bench
        .post(OFFICE_SUMMON_PATH, &json!({"agent": "leader"}))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("확인 답 여섯과 멘션 답이 끝난다", || {
        let store = bench.store();
        let acks = events(&store, pi_office::EVENT_NOTICE_ACK)
            .iter()
            .filter(|e| e["state"] == "answered")
            .count();
        let mention = events(&store, pi_office::EVENT_CHAT)
            .iter()
            .any(|e| e["sourceMessageId"] == "o-1" && e["status"] == "complete");
        acks == 6 && mention
    })
    .await;
    assert_eq!(bench.outside_ai_requests().await, 0, "AI는 가짜 AI만 쓴다");
    assert_eq!(
        bench.outside_order_requests().await,
        0,
        "증권사 주문 요청 없음"
    );
    bench.close().await;
}
