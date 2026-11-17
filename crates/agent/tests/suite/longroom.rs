//! 8차 작업(후보 일괄 처리): 줄이 아주 많은 방에서도 마지막 번호를 바로 알아 줄 id에 넣는다.

use pi_store::{NewRoomEvent, RoomActorKind};
use serde_json::json;

use crate::common::{Env, NOW, request};

// 방 `room-1`에 줄을 `count`개 미리 쌓는다(사용자 글).
fn fill(env: &Env, count: u32) {
    for number in 0..count {
        let event = NewRoomEvent {
            room: "room-1".into(),
            event_id: format!("user-{number}"),
            kind: "message.user".into(),
            actor_kind: RoomActorKind::User,
            actor_id: "me".into(),
            payload: json!({"text": "안녕"}),
            at: NOW,
        };
        env.store.append_room_event(event).unwrap();
    }
}

// 검증: 방 줄 id의 마지막 번호 접두(500줄 넘는 방)
#[tokio::test]
async fn 오백줄_넘는_방에서도_마지막_번호가_줄_id에_들어간다() {
    let env = Env::new();
    let trade = env.trade();
    fill(&env, 1203);
    let mut missing = request(trade);
    missing.data[1].content = None;
    env.team().discuss(missing).await.unwrap();
    let events = env.store.room_events("room-1", 1203, 10).unwrap();
    let last = events.last().unwrap();
    assert_eq!(last.seq, 1204);
    assert_eq!(last.event_id, format!("trade-{}-{}-1203-0", trade.0, NOW.0));
}

// 검증: 방 줄 id의 마지막 번호 접두(store의 마지막 번호 조회를 쓴다, 줄을 읽어 세지 않는다)
#[test]
fn 마지막_번호는_저장소_조회로_알고_줄을_읽어_세지_않는다() {
    let source = include_str!("../../src/roomlog.rs");
    assert!(source.contains("room_last_seq"));
    assert!(!source.contains("room_events"));
}
