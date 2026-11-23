//! 8차 작업(docs/dev/modules/market.md "8차 작업" 1 · 2번).
//! 가격제한 근처 판단의 뺄셈이 넘쳐도 panic 없이 "잴 수 없음 → 가격제한 근처일 수 있다"로 본다.
//! 시각은 테스트가 정한다. 국내 달력: 2026-03-25(수) 정규장 00:00Z~06:30Z.

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, StatusPolicy, SuspectReason,
    Symbol, TradingDay, TradingStatus, estimate_status,
};
use rust_decimal::Decimal;

/// 국내 장중(11:00 KST).
const KR_OPEN: &str = "2026-03-25T02:00:00Z";

fn day(date: &str) -> TradingDay {
    let date: NaiveDate = date.parse().unwrap();
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: date.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            end: date.and_hms_opt(6, 30, 0).unwrap().and_utc(),
        }],
    }
}

fn kr_calendar() -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        previous_business_day: day("2026-03-24"),
        today: day("2026-03-25"),
        next_business_day: day("2026-03-26"),
    }
}

/// 90초 전부터 이어 지켜보고 창 안에서 바뀐 시세들. 마지막(5초 전) 가격만 `last`다.
fn recent(now: DateTime<Utc>, last: Decimal) -> Vec<Quote> {
    let points = [
        (90, Decimal::from(100)),
        (65, Decimal::from(100)),
        (40, Decimal::from(100)),
        (15, Decimal::from(101)),
        (5, last),
    ];
    points
        .into_iter()
        .map(|(ago, price)| Quote {
            symbol: Symbol::new("005930"),
            price,
            currency: Currency::Krw,
            exchange_time: None,
            received_at: now - chrono::Duration::seconds(ago),
        })
        .collect()
}

fn near_limit(reasons: &[SuspectReason]) -> bool {
    reasons.contains(&SuspectReason::NearUpperLimit)
        || reasons.contains(&SuspectReason::NearLowerLimit)
}

// 검증: P12
#[test]
fn 가격에서_기준가를_빼다_넘치면_멈추지_않고_가격제한_근처로_본다() {
    let now: DateTime<Utc> = KR_OPEN.parse().unwrap();
    // 가격 -최대값, 기준가 +최대값: 가격 - 기준가가 Decimal 범위를 넘는다.
    let got = estimate_status(
        &recent(now, Decimal::MIN),
        Some(Decimal::MAX),
        &kr_calendar(),
        now,
        &StatusPolicy::for_market(Market::Kr),
    );
    assert!(near_limit(&got.reasons), "까닭: {:?}", got.reasons);
    assert_eq!(got.status, TradingStatus::Unknown);
}

// 검증: P12
#[test]
fn 가격제한_폭에서_여유를_빼다_넘치면_멈추지_않고_가격제한_근처로_본다() {
    let now: DateTime<Utc> = KR_OPEN.parse().unwrap();
    let mut policy = StatusPolicy::for_market(Market::Kr);
    // 폭 -최대값, 여유 +최대값: band - margin이 Decimal 범위를 넘는다.
    policy.limit_band = Some(Decimal::MIN);
    policy.near_limit_margin = Decimal::MAX;
    let got = estimate_status(
        &recent(now, Decimal::from(101)),
        Some(Decimal::from(100)),
        &kr_calendar(),
        now,
        &policy,
    );
    assert!(near_limit(&got.reasons), "까닭: {:?}", got.reasons);
    assert_eq!(got.status, TradingStatus::Unknown);
}
