//! 하루 동안 비밀이 새지 않는 시험(V47 · E38 · N8 · T18).
//!
//! 시험용 키를 넣고, 가짜 AI가 일부러 그 키 글자를 말하게 하고(논의 · 복기 · 채팅), 사람이 채팅에 키를
//! 붙여 넣은 채로 하루(논의 → 승인 알림 → 매매 → 복기 → 마감 · 하루 정리 → 백업)를 돌린 뒤
//! 기록(이벤트 · 방 로그 · 지식 색인) · 화면(API 응답) · 알림 · AI에 보낸 글에서 그 키 글자를 찾는다.

use std::time::Duration;

use pi_agent::Role;
use pi_store::KnowledgeKind;
use price_inha::{
    DAILY_SUMMARY_PATH, DAY_ROOM, DemoTurn, NOTICES_PATH, OFFICE_CHAT_PATH, OFFICE_PATH,
    PROPOSALS_PATH, TEAM_PATH, TRADE_ROOM_PREFIX,
};
use serde_json::json;

use crate::common::{
    Bench, DART_KEY, HYNIX, events, get, kst, monday, post, subscribe, swing_with, wait_for,
};

// 검증: V47 E38 N8 T18
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시험용_키를_넣고_하루_돌려도_어디에도_그_키_글자가_없다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    subscribe(bench.addr(), 1).await;
    bench.script(
        DemoTurn::Talk(Role::Scout),
        &format!("새 공시를 확인했다 {DART_KEY} @strategy @risk"),
    );
    bench.script(
        DemoTurn::Chat(Role::Risk),
        &format!("키는 {DART_KEY} 입니다"),
    );
    let review = json!({"mistakes": ["late_stop"], "note": format!("손절이 늦었다 {DART_KEY}")});
    bench.script(DemoTurn::Review, &review.to_string());

    bench.ready().await;
    let past = bench.sold(HYNIX).await;
    bench.set(bench.now() + chrono::Duration::minutes(1));
    swing_with(
        bench.addr(),
        json!({"symbol_weight_pct": "10", "auto_order_limit_pct": "2"}),
    )
    .await;
    let discussed = bench.discuss_buy(HYNIX).await;
    bench.push.wait_count(1, Duration::from_secs(10)).await;
    let question = format!("이 키 {DART_KEY} 를 어디에 쓰나요?");
    let body =
        json!({"room": "direct", "agent": "risk", "text": question, "sourceMessageId": "s-1"});
    let reply = post(bench.addr(), OFFICE_CHAT_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text());
    wait_for("채팅 답이 끝난다", || {
        events(&bench.store(), pi_office::EVENT_CHAT)
            .iter()
            .any(|e| e["status"] == "complete" || e["status"] == "failed")
    })
    .await;
    bench.crew().review_day().await.expect("복기");
    bench.set(kst(monday(), 15, 40));
    wait_for("하루 정리 알림", || {
        !events(&bench.store(), "notice.daily_summary").is_empty()
    })
    .await;
    bench.app.back_up_now().await.expect("백업");

    // 기록: 저장된 이벤트 전부 · 방 로그 · 지식 색인.
    let store = bench.store();
    let all = store.read_after(0, 100_000).unwrap();
    assert!(
        all.iter().any(|e| e.kind.starts_with("notice.")),
        "알림이 있었다"
    );
    for event in &all {
        assert!(
            !event.payload.to_string().contains(DART_KEY),
            "이벤트 {} {}에 키 글자",
            event.seq,
            event.kind
        );
    }
    let rooms = [
        format!("{TRADE_ROOM_PREFIX}{}", discussed.trade.0),
        DAY_ROOM.to_string(),
    ];
    for room in rooms {
        for line in store.room_events(&room, 0, 10_000).unwrap() {
            assert!(
                !line.payload.to_string().contains(DART_KEY),
                "방 {room}에 키 글자"
            );
        }
    }
    for kind in [KnowledgeKind::Trade, KnowledgeKind::Review] {
        for chunk in store.knowledge_chunks(kind, &past.0.to_string()).unwrap() {
            assert!(!chunk.text.contains(DART_KEY), "지식 색인에 키 글자");
        }
    }
    // AI에 보낸 글.
    let prompts = bench.ai().prompts();
    assert!(
        prompts.iter().any(|p| p.contains("어디에 쓰나요")),
        "채팅 질문은 AI로 갔다"
    );
    assert!(
        prompts.iter().all(|p| !p.contains(DART_KEY)),
        "AI에 보낸 글에 키 글자"
    );
    // 화면: 사무실 · 알림 목록 · 하루 정리 · 팀 · 개선안.
    for path in [
        OFFICE_PATH.to_string(),
        format!("{NOTICES_PATH}?after=0&max=200"),
        DAILY_SUMMARY_PATH.to_string(),
        TEAM_PATH.to_string(),
        PROPOSALS_PATH.to_string(),
    ] {
        let reply = get(bench.addr(), &path).await;
        assert_eq!(reply.status, 200, "{path}");
        assert!(!reply.text().contains(DART_KEY), "{path} 응답에 키 글자");
    }
    bench.close().await;
}
