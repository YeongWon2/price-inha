//! 1번: 키도 종목도 없는 새 홈에서 `run --demo`를 켜면 시연용 기본 종목 몇 개를 조립 값으로 지켜본다. 파일
//! (`WATCHLIST_FILE`)에는 쓰지 않고(E56), 설정 상태의 `symbols_source`는 `"demo"`다. 종목을 등록하면 그것으로 바뀐다.

use pi_market::{Market, Symbol};
use price_inha::{
    ITEM_SYMBOLS, SETUP_PATH, SETUP_SYMBOLS_PATH, WATCHLIST_FILE, demo_symbols, run_assembly,
    save_watchlist,
};
use serde_json::{Value, json};

use crate::common::{Running, get, post, run_settings};

fn texts(symbols: &[Symbol]) -> Vec<String> {
    symbols.iter().map(|s| s.as_str().to_string()).collect()
}

fn symbols_registered(status: &Value) -> bool {
    status["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == json!(ITEM_SYMBOLS))
        .map(|item| item["registered"] == json!(true))
        .unwrap()
}

// 검증: V32 E56 D14
#[test]
fn 시연_기본_종목은_시장마다_여럿이고_첫째는_지금의_시연_종목이다() {
    for (market, first) in [(Market::Kr, "005930"), (Market::Us, "AAPL")] {
        let symbols = demo_symbols(market);
        assert!(symbols.len() >= 3, "기본 종목이 여럿이다: {symbols:?}");
        assert_eq!(symbols[0].as_str(), first, "첫째는 지금의 시연 종목이다");
        let mut unique = texts(&symbols);
        unique.dedup();
        assert_eq!(unique.len(), symbols.len(), "겹치지 않는다");
    }
}

// 검증: D14 E56 P8
#[test]
fn 종목_없는_시연_조립은_기본_종목마다_시연_업종을_넣는다() {
    let home = tempfile::tempdir().unwrap();
    let demo = run_assembly(&run_settings(home.path(), true), None);
    for symbol in demo_symbols(Market::Kr) {
        assert!(
            demo.launch.trading.rules.sectors.contains_key(&symbol),
            "기본 종목 {symbol:?}에 시연 업종이 있다"
        );
    }
    let real = run_assembly(&run_settings(home.path(), false), None);
    assert!(
        real.launch.trading.rules.sectors.is_empty(),
        "실제 조립에는 시연 값이 섞이지 않는다(E56)"
    );
    assert!(
        !home.path().join(WATCHLIST_FILE).exists(),
        "파일에 쓰지 않는다"
    );
}

// 검증: V32 E56 D14
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 키도_종목도_없는_시연은_기본_종목을_지켜보고_출처는_demo이며_파일에_쓰지_않는다() {
    let home = tempfile::tempdir().unwrap();
    let running = Running::start(home.path(), true).await;
    let status = get(running.addr(), SETUP_PATH).await;
    assert_eq!(status.status, 200, "{}", status.text);
    assert_eq!(
        status.body["symbols"],
        json!(texts(&demo_symbols(Market::Kr))),
        "{}",
        status.text
    );
    assert_eq!(
        status.body["symbols_source"],
        json!("demo"),
        "{}",
        status.text
    );
    assert!(
        !symbols_registered(&status.body),
        "시연 기본 종목은 등록한 것이 아니다: {}",
        status.text
    );
    assert!(
        !home.path().join(WATCHLIST_FILE).exists(),
        "시연 기본 종목은 파일에 쓰지 않는다(E56)"
    );

    // 종목을 등록하면 그것으로 바뀌고 출처는 저장된 것이다.
    let reply = post(
        running.addr(),
        SETUP_SYMBOLS_PATH,
        &json!({"symbols": ["000660"]}),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let status = get(running.addr(), SETUP_PATH).await;
    assert_eq!(status.body["symbols"], json!(["000660"]), "{}", status.text);
    assert_eq!(
        status.body["symbols_source"],
        json!("saved"),
        "{}",
        status.text
    );
    running.assert_stayed_home().await;
    running.close().await;
}

// 검증: V32 V31
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 저장된_종목이_있는_시연은_그것을_쓰고_기본_종목을_섞지_않는다() {
    let home = tempfile::tempdir().unwrap();
    save_watchlist(home.path(), &[Symbol::new("000660")]).unwrap();
    let running = Running::start(home.path(), true).await;
    let status = get(running.addr(), SETUP_PATH).await;
    assert_eq!(status.body["symbols"], json!(["000660"]), "{}", status.text);
    assert_eq!(
        status.body["symbols_source"],
        json!("saved"),
        "{}",
        status.text
    );
    running.close().await;
}
