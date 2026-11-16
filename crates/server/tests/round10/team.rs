//! 담당 6명 · 팀 논의 · 점수 · 팀장 판단 · 주문 안 잇기(T9, V1~V4 · V17 · E27 · E32).

use pi_agent::{Decision, Role, WatchReason};
use pi_store::{RoomEvent, TradeState};
use price_inha::{DemoTurn, Placed, TEAM_MEMBERS_PATH};
use serde_json::json;

use crate::common::*;

/// 그 담당이 쓴 방 글들(`message.member`)의 내용.
fn said_by(room: &[RoomEvent], role: &str) -> Vec<serde_json::Value> {
    room.iter()
        .filter(|event| event.kind == "message.member" && event.actor_id == role)
        .map(|event| event.payload.clone())
        .collect()
}

// 검증: V1, T9
#[tokio::test]
async fn 담당_명단에_여섯_명이_명단_순서로_보인다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let addr = bench.addr();

    let reply = get(addr, TEAM_MEMBERS_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let members = reply.body["members"].as_array().expect("명단").clone();
    let ids: Vec<&str> = members.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["scout", "strategy", "risk", "review", "trader", "leader"],
        "뉴스 공시 시세 · 전략 · 리스크 · 복기 · 매매 담당과 팀장"
    );
    for member in &members {
        let name = member["name"].as_str().unwrap_or_default();
        assert!(!name.trim().is_empty(), "이름이 있다: {member}");
    }

    let anonymous = get_anonymous(addr, TEAM_MEMBERS_PATH).await;
    assert_eq!(anonymous.status, 401, "세션이 없으면 열리지 않는다");
    bench.close().await;
}

// 검증: V2, T9, T10
#[tokio::test]
async fn 관심_종목에_새_공시가_오면_담당이_관련_담당에게_넘긴_기록이_남는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.script(DemoTurn::Verdict, &watch_verdict());

    let discussed = bench.crew().scan().await.expect("공시를 본다");
    assert_eq!(discussed.len(), 1, "지켜보는 종목 하나에 새 공시가 있다");
    let trade = discussed[0].trade;
    assert_eq!(bench.store().get_trade(trade).unwrap().symbol, WATCHED);
    assert!(
        bench
            .data
            .paths()
            .await
            .iter()
            .any(|p| p == "/api/list.json"),
        "공시를 로컬 가짜 서버에서 받았다"
    );

    let room = bench.room(trade);
    let scout = said_by(&room, "scout");
    assert!(!scout.is_empty(), "뉴스 공시 시세 담당이 먼저 말한다");
    assert_eq!(
        scout[0]["handoff"],
        json!(["strategy", "risk"]),
        "관련 담당(전략 · 리스크)에게 넘겼다"
    );
    assert!(
        !said_by(&room, "strategy").is_empty() && !said_by(&room, "risk").is_empty(),
        "넘겨받은 담당이 답했다"
    );

    let again = bench.crew().scan().await.expect("다시 본다");
    assert!(
        again.is_empty(),
        "같은 공시(접수번호)는 두 번 논의하지 않는다"
    );
    bench.close().await;
}

// 검증: V2, E32
#[tokio::test]
async fn 공시가_없으면_논의하지_않는다() {
    let dir = tempfile::tempdir().unwrap();
    let bench = Bench::with(dir, kst(monday(), 10, 0), Some(Gap::NoDisclosure)).await;

    let discussed = bench.crew().scan().await.expect("공시를 본다");
    assert!(discussed.is_empty(), "새 공시가 없으면 깨울 일이 없다");
    let discussing = bench
        .store()
        .trades_in_state(TradeState::Discussing)
        .unwrap();
    assert!(discussing.is_empty(), "논의중 매매를 만들지 않는다");
    bench.close().await;
}

// 검증: V3, V4, T9, M2
#[tokio::test]
async fn 사기로_정하면_점수와_판단이_남고_주문_안이_주문_길로_간다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;

    let discussed = bench.discuss_buy(HYNIX).await;
    assert_eq!(discussed.outcome.decision, Decision::Buy);
    let trade = bench.store().get_trade(discussed.trade).unwrap();
    let mut roles: Vec<&str> = trade.scores.iter().map(|s| s.role.as_str()).collect();
    roles.sort_unstable();
    assert_eq!(
        roles,
        ["risk", "scout", "strategy"],
        "팀장을 뺀 참가 담당의 점수"
    );
    assert_eq!(
        trade.consensus_score,
        Some(210),
        "합의 점수는 점수의 합(70 × 3)"
    );
    assert!(trade.decided_at.is_some(), "팀장의 판단 시각이 남는다");
    let leader = said_by(&bench.room(discussed.trade), "leader");
    assert!(
        leader.iter().any(|p| p["decision"] == json!("buy")),
        "팀장의 종목 판단이 방 로그에 남는다"
    );

    let draft = discussed
        .outcome
        .order
        .as_ref()
        .expect("매매 담당의 주문 안");
    assert_eq!(draft.symbol, HYNIX);
    assert!(
        matches!(discussed.placed, Some(Placed::Ordered(_))),
        "한도 안의 주문 안은 주문 길을 지나 나간다: {:?}",
        discussed.placed
    );
    assert_eq!(bench.trade_state(discussed.trade), TradeState::Ordered);
    assert_eq!(bench.orders().await.len(), 1, "모의계좌에 주문 하나");
    bench.close().await;
}

// 검증: E27, V4
#[tokio::test]
async fn 합의_점수가_기준을_넘지_못하면_지켜보고_주문_안이_없다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    for role in [Role::Scout, Role::Strategy, Role::Risk] {
        bench.script(
            DemoTurn::Score(role),
            r#"{"score": 60, "reason": "애매하다"}"#,
        );
    }

    let discussed = bench.discuss_buy(HYNIX).await;
    assert!(
        matches!(
            discussed.outcome.decision,
            Decision::Watch(WatchReason::ConsensusBelow {
                consensus: 180,
                required: 180
            })
        ),
        "같으면 넘지 못한 것이다: {:?}",
        discussed.outcome.decision
    );
    assert!(discussed.outcome.order.is_none() && discussed.placed.is_none());
    assert_eq!(bench.trade_state(discussed.trade), TradeState::Discussing);
    assert!(bench.orders().await.is_empty(), "주문이 나가지 않는다");
    bench.close().await;
}

// 검증: V17, P9
#[tokio::test]
async fn 손절값을_뺀_판단은_막히고_주문이_나가지_않는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let no_stop = json!({
        "decision": "buy", "hypothesis": "가설", "trigger": "계기",
        "entry": ENTRY, "target": 110_000, "expected_value": 5_000
    });

    bench.quote(HYNIX, ENTRY, bench.now()).await;
    bench.script(DemoTurn::Verdict, &no_stop.to_string());
    let discussed = bench
        .crew()
        .discuss(HYNIX, "손절값 없는 판단")
        .await
        .expect("논의가 끝나야 한다");
    assert_eq!(
        discussed.outcome.decision,
        Decision::Watch(WatchReason::RationaleIncomplete)
    );
    assert!(discussed.outcome.order.is_none() && discussed.placed.is_none());
    let blocked = bench.room(discussed.trade).iter().any(|event| {
        event.kind == "room.activity"
            && event.payload["reason_code"] == json!("rationale_incomplete")
    });
    assert!(blocked, "막힌 까닭이 방 로그에 남는다");
    assert!(bench.orders().await.is_empty(), "주문이 나가지 않는다");
    bench.close().await;
}

// 검증: E32, T10
#[tokio::test]
async fn 금리_자료가_비면_지켜보고_주문_안이_없다() {
    let dir = tempfile::tempdir().unwrap();
    let bench = Bench::with(dir, kst(monday(), 10, 0), Some(Gap::NoRate)).await;
    bench.ready().await;

    let discussed = bench.discuss_buy(HYNIX).await;
    match &discussed.outcome.decision {
        Decision::Watch(WatchReason::MissingData(names)) => {
            assert_eq!(names.len(), 1, "빈 자료 하나: {names:?}");
        }
        other => panic!("자료가 비면 지켜보기다: {other:?}"),
    }
    assert!(discussed.placed.is_none());
    assert!(bench.orders().await.is_empty());
    bench.close().await;
}
