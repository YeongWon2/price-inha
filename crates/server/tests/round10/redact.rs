//! 방 로그 · 세션 · 복기 기록에는 가린 글만 남는다(E38 · N8 · M7, agent · knowledge 1차 검토 참고).

use pi_agent::Role;
use pi_journal::Journal;
use price_inha::DemoTurn;

use crate::common::*;

// 검증: E38, N8, T9
#[tokio::test]
async fn 담당의_답에_비밀이_섞여도_방_로그와_세션에는_가린_글만_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let leak = format!("키는 {DART_KEY} 이다 @strategy @risk");
    bench.script(DemoTurn::Talk(Role::Scout), &leak);
    bench.script(DemoTurn::Verdict, &watch_verdict());

    bench.quote(HYNIX, ENTRY, bench.now()).await;
    let discussed = bench.crew().discuss(HYNIX, "비밀 섞인 글").await.unwrap();

    let room = bench.room(discussed.trade);
    let scout = room
        .iter()
        .find(|e| e.kind == "message.member" && e.actor_id == "scout")
        .expect("뉴스 공시 시세 담당의 글");
    let text = scout.payload["text"].as_str().unwrap();
    assert!(!text.contains(DART_KEY), "방 로그에 비밀이 없다: {text}");
    assert!(text.contains("키는"), "가린 글은 남는다");
    for event in &room {
        assert!(!event.payload.to_string().contains(DART_KEY));
    }

    let store = bench.store();
    for session in store.sessions_of("scout").unwrap() {
        for message in store.session_messages(session.id).unwrap() {
            let content = message.content.unwrap_or_default();
            let api = message.api_content.unwrap_or_default();
            assert!(
                !content.contains(DART_KEY) && !api.contains(DART_KEY),
                "세션에 비밀이 없다"
            );
        }
    }
    bench.close().await;
}

// 검증: E38, M7, T15
#[tokio::test]
async fn 복기_글에_비밀이_섞여도_복기_기록에는_가린_글만_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    bench.sold(HYNIX).await;
    let note = format!("손절이 늦었다 {DART_KEY}");
    let reply = serde_json::json!({"mistakes": ["late_stop"], "note": note}).to_string();
    bench.script(DemoTurn::Review, &reply);

    bench.crew().review_day().await.unwrap();
    let reviews = Journal::new(bench.store()).reviews().unwrap();
    assert_eq!(reviews.len(), 1);
    assert!(
        !reviews[0].note.contains(DART_KEY),
        "복기 기록에 비밀이 없다"
    );
    assert!(reviews[0].note.contains("손절이 늦었다"));
    bench.close().await;
}
