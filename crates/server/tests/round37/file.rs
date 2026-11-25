//! 35차 1번: 보관 폴더의 금지 종목 · 업종표 파일을 켤 때 읽어 주문 전 점검(P5 · P8)에 쓴다.

use price_inha::{AppError, RULES_CONFIG_FILE, RULES_SECTOR_MAX_CHARS};

use crate::common::{Bench, HYNIX, blocked_detail, config_file, kst, launch_error, monday, passed};

// 검증: P8 V26
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 파일이_없으면_업종표가_비어_사는_주문은_업종표에_없는_종목으로_막힌다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    let (_trade, placed) = bench.buy(HYNIX).await;
    let detail = blocked_detail(&placed).expect("업종을 모르면 사는 주문을 막는다");
    assert!(
        detail.contains("P8"),
        "업종표에 없는 종목은 P8로 막는다: {detail}"
    );
    assert!(
        !bench.home().join(RULES_CONFIG_FILE).exists(),
        "켜기만 해서는 파일을 만들지 않는다"
    );
    bench.close().await;
}

// 검증: P8 R7-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 파일의_업종표에_있는_종목은_업종_점검을_지난다() {
    let raw = config_file(&[], &[(HYNIX, "반도체")]);
    let bench = Bench::with_file(kst(monday(), 10, 0), &raw).await;
    bench.ready().await;
    let (_trade, placed) = bench.buy(HYNIX).await;
    assert!(
        passed(&placed),
        "업종을 아는 종목은 주문 전 점검을 지나 주문(또는 승인 기다림)이 된다: {placed:?}"
    );
    bench.close().await;
}

// 검증: P5 R7-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 파일의_금지_종목은_업종이_있어도_금지_종목으로_막힌다() {
    let raw = config_file(&[("kr", HYNIX)], &[(HYNIX, "반도체")]);
    let bench = Bench::with_file(kst(monday(), 10, 0), &raw).await;
    bench.ready().await;
    let (trade, placed) = bench.buy(HYNIX).await;
    let detail = blocked_detail(&placed).expect("금지 종목은 사는 주문을 막는다");
    assert!(detail.contains("P5"), "금지 종목은 P5로 막는다: {detail}");
    let saved = bench.store().get_trade(trade).unwrap();
    assert_eq!(
        saved.state,
        pi_store::TradeState::Blocked,
        "그 매매는 막힘이고 까닭이 남는다"
    );
    bench.close().await;
}

// 검증: P8 R7-3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 업종_이름은_앞뒤_공백을_떼고_가장_긴_글자_수까지_받는다() {
    let longest = "가".repeat(RULES_SECTOR_MAX_CHARS);
    let raw = config_file(&[], &[(HYNIX, &format!("  {longest}  "))]);
    let bench = Bench::with_file(kst(monday(), 10, 0), &raw).await;
    bench.ready().await;
    let (_trade, placed) = bench.buy(HYNIX).await;
    assert!(passed(&placed), "경계 길이는 받는다: {placed:?}");
    bench.close().await;
}

/// 깨진 파일 하나로 켜 보면 켜지지 않고 그 파일 이름으로 까닭을 말한다.
async fn refuses(raw: &str, why: &str) {
    match launch_error(raw).await {
        AppError::BadFile { file, .. } => {
            assert_eq!(file, RULES_CONFIG_FILE, "{why}: 그 파일이 까닭이다");
        }
        other => panic!("{why}: 깨진 파일이라는 까닭이어야 한다: {other}"),
    }
}

// 검증: P5 P8 E56
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 깨진_파일이면_켜지지_않고_그_파일이_까닭이라고_말한다() {
    refuses("{\"banned\": [", "JSON이 아니다").await;
    refuses("{\"banned\": 3, \"sectors\": {}}", "모양이 다르다").await;
    refuses("[]", "객체가 아니다").await;
    let bad_symbol = config_file(&[("kr", "삼성 전자")], &[]);
    refuses(&bad_symbol, "종목 글자가 아니다").await;
    let bad_market = config_file(&[("jp", HYNIX)], &[]);
    refuses(&bad_market, "모르는 시장").await;
    let twice = config_file(&[("kr", HYNIX), ("kr", HYNIX)], &[]);
    refuses(&twice, "같은 금지 종목이 두 번").await;
    let blank = config_file(&[], &[(HYNIX, "   ")]);
    refuses(&blank, "업종 이름이 비었다").await;
    let long = config_file(&[], &[(HYNIX, &"가".repeat(RULES_SECTOR_MAX_CHARS + 1))]);
    refuses(&long, "업종 이름이 너무 길다").await;
    let bad_key = config_file(&[], &[("삼성", "반도체")]);
    refuses(&bad_key, "업종표의 종목 글자가 아니다").await;
}
