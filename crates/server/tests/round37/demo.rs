//! 35차 4번: 시연 조립은 지켜보는 종목에 시연 업종을 조립 값으로 넣고(파일에 쓰지 않는다), 시연 어댑터는 VI ·
//! 거래정지를 "아님"으로 알린다. 그래서 시연에서 팀이 사기로 정하면 주문 전 점검을 지나 주문(또는 승인
//! 기다림)까지 간다 — 모의계좌뿐이고 실제 증권사로는 나가지 않는다.

use pi_market::Symbol;
use pi_store::TradeState;
use price_inha::{KIND_OFFICE_APPROVAL, RULES_CONFIG_FILE, run_assembly};
use serde_json::json;

use crate::common::{
    Demo, LONG, WATCHED, events, kst, run_settings, saturday, wait_until, watched_home,
};

// 검증: D14 E56 P8 C33
#[test]
fn 시연_조립은_지켜보는_종목마다_시연_업종을_조립_값으로_넣고_파일은_쓰지_않는다() {
    let home = watched_home();
    let demo = run_assembly(&run_settings(home.path(), true), None);
    let sector = demo
        .launch
        .trading
        .rules
        .sectors
        .get(&Symbol::new(WATCHED))
        .cloned()
        .expect("지켜보는 종목에 시연 업종이 있다");
    assert!(
        !sector.trim().is_empty(),
        "시연 업종 이름이 있다: {sector:?}"
    );
    assert!(
        demo.launch.trading.rules.banned_symbols.is_empty(),
        "시연 조립은 금지 종목을 만들지 않는다"
    );
    assert!(
        !home.path().join(RULES_CONFIG_FILE).exists(),
        "시연 업종은 설정 파일에 쓰지 않는다(E56)"
    );

    let real = run_assembly(&run_settings(home.path(), false), None);
    assert!(
        real.launch.trading.rules.sectors.is_empty(),
        "실제 조립에는 시연 업종이 섞이지 않는다(E56)"
    );
}

// 검증: D14 C33 P8 P12 V54 E56 R2-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연에서_사기로_정하면_막힘이_아니라_주문이나_승인_기다림까지_가고_바깥으로_나가지_않는다()
{
    let bench = Demo::start(kst(saturday(), 14, 0)).await;
    bench.ready().await;
    let discussed = bench
        .crew()
        .discuss(WATCHED, "35차 시연 주문")
        .await
        .expect("논의가 끝까지 돈다");
    let store = bench.store();
    let past_check = [
        TradeState::Ordered,
        TradeState::Holding,
        TradeState::Sold,
        TradeState::Reviewed,
        TradeState::AwaitingApproval,
    ];
    wait_until("주문 전 점검을 지난다", LONG, || {
        let trade = store.get_trade(discussed.trade).unwrap();
        assert_ne!(
            trade.state,
            TradeState::Blocked,
            "시연에서 사기로 정한 매매가 막히면 안 된다: {:?}",
            trade.blocked
        );
        past_check.contains(&trade.state)
    })
    .await;
    let trade = store.get_trade(discussed.trade).unwrap();
    assert!(
        trade.blocked.is_none(),
        "막힌 까닭이 없다: {:?}",
        trade.blocked
    );
    if trade.state == TradeState::AwaitingApproval {
        wait_until(
            "승인을 기다리면 사무실 승인 이벤트가 난다",
            LONG,
            || {
                events(&store, KIND_OFFICE_APPROVAL).iter().any(|card| {
                    card["agent"] == json!("trader") && card["state"] == json!("waiting")
                })
            },
        )
        .await;
    }
    assert!(
        !bench.home().join(RULES_CONFIG_FILE).exists(),
        "시연 업종은 설정 파일에 쓰지 않는다(E56)"
    );
    for (name, count) in bench.outside.counts().await {
        assert_eq!(count, 0, "시연 모드는 실제 {name} 주소로 요청하지 않는다");
    }
    bench.close().await;
}
