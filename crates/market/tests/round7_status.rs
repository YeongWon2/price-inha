//! 7차 작업(docs/dev/modules/market.md "7차 작업" 3 · 4 · 5번, 결정 2026-11-21 5 · 11 · 13번).
//! 시세로 거래 상태(VI · 거래정지)를 추정하고, 공급자 상태와 합친다. 순수 함수만 본다.
//! 시각은 테스트가 정한다.
//!
//! 국내 달력: 2026-03-25(수) 정규장 09:00~15:30 KST = 00:00Z~06:30Z, 앞뒤 영업일도 같다.
//! 미국 달력: 2026-03-25(수) 정규장 09:30~16:00 EDT = 13:30Z~20:00Z, 앞뒤 영업일도 같다.

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, StalePolicy, StatusEstimate,
    StatusPolicy, StatusSource, SuspectReason, Symbol, TradingDay, TradingStatus, estimate_status,
    resolve_status,
};
use rust_decimal::Decimal;

/// 국내 장중(11:00 KST).
const KR_OPEN: &str = "2026-03-25T02:00:00Z";
/// 국내 장 마감 뒤(16:00 KST). 달력은 덮는다.
const KR_CLOSED: &str = "2026-03-25T07:00:00Z";
/// 국내 달력이 덮지 않는 시각(다음 영업일 뒤).
const KR_OUTSIDE: &str = "2026-03-28T02:00:00Z";
/// 미국 장중(11:00 EDT).
const US_OPEN: &str = "2026-03-25T15:00:00Z";

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

fn day(date: &str, start_hm: (u32, u32), end_hm: (u32, u32)) -> TradingDay {
    let date: NaiveDate = date.parse().unwrap();
    let time = |(h, m): (u32, u32)| date.and_hms_opt(h, m, 0).unwrap().and_utc();
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: time(start_hm),
            end: time(end_hm),
        }],
    }
}

fn kr_calendar() -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        previous_business_day: day("2026-03-24", (0, 0), (6, 30)),
        today: day("2026-03-25", (0, 0), (6, 30)),
        next_business_day: day("2026-03-26", (0, 0), (6, 30)),
    }
}

fn us_calendar() -> MarketCalendar {
    MarketCalendar {
        market: Market::Us,
        previous_business_day: day("2026-03-24", (13, 30), (20, 0)),
        today: day("2026-03-25", (13, 30), (20, 0)),
        next_business_day: day("2026-03-26", (13, 30), (20, 0)),
    }
}

/// `now`보다 `ago`초 앞에 받은 시세.
fn q(symbol: &str, price: &str, now: DateTime<Utc>, ago: i64) -> Quote {
    Quote {
        symbol: Symbol::new(symbol),
        price: d(price),
        currency: Currency::Krw,
        exchange_time: None,
        received_at: now - chrono::Duration::seconds(ago),
    }
}

/// 한 종목의 시세들: `(몇 초 전, 가격)`.
fn series(symbol: &str, now: DateTime<Utc>, points: &[(i64, &str)]) -> Vec<Quote> {
    points
        .iter()
        .map(|(ago, price)| q(symbol, price, now, *ago))
        .collect()
}

fn kr(points: &[(i64, &str)], reference: Option<&str>) -> StatusEstimate {
    kr_with(points, reference, &StatusPolicy::for_market(Market::Kr))
}

fn kr_with(
    points: &[(i64, &str)],
    reference: Option<&str>,
    policy: &StatusPolicy,
) -> StatusEstimate {
    let now = at(KR_OPEN);
    estimate_status(
        &series("005930", now, points),
        reference.map(d),
        &kr_calendar(),
        now,
        policy,
    )
}

/// 정상인 국내 시세: 90초 전부터 지켜봤고, 30초 전에 가격이 바뀌었고, 5초 전에 받았다.
const NORMAL: &[(i64, &str)] = &[(90, "100"), (30, "101"), (5, "101")];

fn normal() -> StatusEstimate {
    StatusEstimate {
        status: TradingStatus::Normal,
        reasons: Vec::new(),
    }
}

fn unknown(reasons: &[SuspectReason]) -> StatusEstimate {
    StatusEstimate {
        status: TradingStatus::Unknown,
        reasons: reasons.to_vec(),
    }
}

// ---------------------------------------------------------------- 추정: 정상

// 검증: P12, N6
#[test]
fn 늦지_않고_움직이고_가격제한에서_멀면_정상이다() {
    assert_eq!(kr(NORMAL, Some("100")), normal());
}

// 검증: P12
#[test]
fn 육십초_안에_한_번이라도_바뀌면_정상이다() {
    let points = [(90, "100"), (50, "100"), (20, "101"), (5, "101")];
    assert_eq!(kr(&points, Some("100")), normal());
}

// ---------------------------------------------------------------- 추정: 까닭마다 하나

// 검증: P12
#[test]
fn 시세가_없으면_모른다() {
    let now = at(KR_OPEN);
    let got = estimate_status(
        &[],
        Some(d("100")),
        &kr_calendar(),
        now,
        &StatusPolicy::for_market(Market::Kr),
    );
    assert_eq!(got.status, TradingStatus::Unknown);
    assert!(got.reasons.contains(&SuspectReason::NoQuote), "{got:?}");
}

// 검증: P12
#[test]
fn 다른_종목이_섞이면_모른다() {
    let now = at(KR_OPEN);
    let mut recent = series("005930", now, NORMAL);
    recent.push(q("000660", "101", now, 3));
    let got = estimate_status(
        &recent,
        Some(d("100")),
        &kr_calendar(),
        now,
        &StatusPolicy::for_market(Market::Kr),
    );
    assert_eq!(got.status, TradingStatus::Unknown);
    assert!(
        got.reasons.contains(&SuspectReason::MixedSymbols),
        "{got:?}"
    );
}

// 검증: P12
#[test]
fn 장이_닫혀_있으면_모른다() {
    let now = at(KR_CLOSED);
    let got = estimate_status(
        &series("005930", now, NORMAL),
        Some(d("100")),
        &kr_calendar(),
        now,
        &StatusPolicy::for_market(Market::Kr),
    );
    assert_eq!(got, unknown(&[SuspectReason::MarketClosed]));
}

// 검증: P12, N6
#[test]
fn 달력이_지금을_덮지_않으면_늦은_것으로_보고_모른다() {
    let now = at(KR_OUTSIDE);
    let got = estimate_status(
        &series("005930", now, NORMAL),
        Some(d("100")),
        &kr_calendar(),
        now,
        &StatusPolicy::for_market(Market::Kr),
    );
    assert_eq!(got.status, TradingStatus::Unknown);
    assert!(got.reasons.contains(&SuspectReason::Stale), "{got:?}");
}

// 검증: P12, N6
#[test]
fn 마지막_시세가_삼십초를_넘으면_늦어서_모른다() {
    let points = [(90, "100"), (31, "101")];
    assert_eq!(kr(&points, Some("100")), unknown(&[SuspectReason::Stale]));
}

// 검증: P12, N6
#[test]
fn 마지막_시세가_딱_삼십초면_아직_늦지_않다() {
    let points = [(90, "100"), (30, "101")];
    assert_eq!(kr(&points, Some("100")), normal());
}

// 검증: P12
#[test]
fn 충분히_지켜보지_않았으면_모른다() {
    // 60초보다 앞에 받은 시세가 없다(장이 막 열렸거나 막 지켜보기 시작했다).
    let points = [(50, "100"), (10, "101")];
    assert_eq!(
        kr(&points, Some("100")),
        unknown(&[SuspectReason::NotObservedLongEnough])
    );
}

// 검증: P12
#[test]
fn 육십초_동안_가격이_같으면_모른다() {
    let points = [(90, "100"), (40, "100"), (5, "100")];
    assert_eq!(
        kr(&points, Some("100")),
        unknown(&[SuspectReason::NotMoving])
    );
}

// 검증: P12
#[test]
fn 육십초_전에만_바뀌고_그_뒤로_그대로면_모른다() {
    let points = [(120, "99"), (90, "100"), (30, "100"), (5, "100")];
    assert_eq!(
        kr(&points, Some("100")),
        unknown(&[SuspectReason::NotMoving])
    );
}

// 검증: P12
#[test]
fn 국내_기준가가_없으면_모른다() {
    assert_eq!(
        kr(NORMAL, None),
        unknown(&[SuspectReason::NoReferencePrice])
    );
}

// 검증: P12
#[test]
fn 국내_등락률이_플러스_이십팔퍼센트면_상한가_근처다() {
    let points = [(90, "127"), (5, "128")];
    assert_eq!(
        kr(&points, Some("100")),
        unknown(&[SuspectReason::NearUpperLimit])
    );
}

// 검증: P12
#[test]
fn 국내_등락률이_이십팔퍼센트를_넘으면_상한가_근처다() {
    let points = [(90, "128"), (5, "130")];
    assert_eq!(
        kr(&points, Some("100")),
        unknown(&[SuspectReason::NearUpperLimit])
    );
}

// 검증: P12
#[test]
fn 국내_등락률이_마이너스_이십팔퍼센트면_하한가_근처다() {
    let points = [(90, "73"), (5, "72")];
    assert_eq!(
        kr(&points, Some("100")),
        unknown(&[SuspectReason::NearLowerLimit])
    );
}

// 검증: P12
#[test]
fn 국내_등락률이_마이너스_이십팔퍼센트_아래면_하한가_근처다() {
    let points = [(90, "72"), (5, "70")];
    assert_eq!(
        kr(&points, Some("100")),
        unknown(&[SuspectReason::NearLowerLimit])
    );
}

// 검증: P12
#[test]
fn 국내_등락률이_이십칠점구구퍼센트면_정상이다() {
    let up = [(90, "127"), (5, "127.99")];
    assert_eq!(kr(&up, Some("100")), normal());
    let down = [(90, "73"), (5, "72.01")];
    assert_eq!(kr(&down, Some("100")), normal());
}

// 검증: P12
#[test]
fn 미국은_기준가가_없어도_큰_등락도_정상이다() {
    let now = at(US_OPEN);
    let recent = series("AAPL", now, &[(90, "100"), (5, "180")]);
    let policy = StatusPolicy::for_market(Market::Us);
    assert_eq!(
        estimate_status(&recent, None, &us_calendar(), now, &policy),
        normal()
    );
    assert_eq!(
        estimate_status(&recent, Some(d("100")), &us_calendar(), now, &policy),
        normal(),
        "미국은 가격제한 검사를 건너뛴다"
    );
}

// 검증: P12, N6
#[test]
fn 받은_시각으로_순서를_맞춘다() {
    // 받은 시각 순: 90초 전 128 → 30초 전 127 → 5초 전 100. 마지막 가격은 100(등락 0%)이다.
    // 넘긴 순서의 끝(90초 전 128)을 마지막으로 보면 늦음 · 상한가 근처로 잘못 본다.
    let points = [(5, "100"), (30, "127"), (90, "128")];
    assert_eq!(kr(&points, Some("100")), normal());
}

// 검증: P12, N6
#[test]
fn 까닭이_여럿이면_모두_enum_순서로_담는다() {
    // 마지막 시세가 40초 전(늦음), 60초 안에 가격이 그대로(멈춤), 기준가 없음.
    let points = [(90, "100"), (40, "100")];
    assert_eq!(
        kr(&points, None),
        unknown(&[
            SuspectReason::Stale,
            SuspectReason::NotMoving,
            SuspectReason::NoReferencePrice,
        ])
    );
}

// ---------------------------------------------------------------- 기준값

// 검증: P12, N6
#[test]
fn 시장별_기본값은_삼십초_육십초_삼십퍼센트_이퍼센트다() {
    assert_eq!(
        StatusPolicy::for_market(Market::Kr),
        StatusPolicy {
            stale: StalePolicy::default(),
            still_after: Duration::from_secs(60),
            limit_band: Some(d("0.30")),
            near_limit_margin: d("0.02"),
        }
    );
    assert_eq!(
        StatusPolicy::for_market(Market::Us),
        StatusPolicy {
            stale: StalePolicy::default(),
            still_after: Duration::from_secs(60),
            limit_band: None,
            near_limit_margin: d("0.02"),
        }
    );
    assert_eq!(StalePolicy::default().max_age, Duration::from_secs(30));
}

// 검증: P12
#[test]
fn 지켜보기_시간을_바꾸면_판단이_따라_바뀐다() {
    let policy = StatusPolicy {
        still_after: Duration::from_secs(120),
        ..StatusPolicy::for_market(Market::Kr)
    };
    assert_eq!(
        kr_with(NORMAL, Some("100"), &policy),
        unknown(&[SuspectReason::NotObservedLongEnough])
    );
}

// 검증: P12, N6
#[test]
fn 늦음_기준을_바꾸면_판단이_따라_바뀐다() {
    let policy = StatusPolicy {
        stale: StalePolicy {
            max_age: Duration::from_secs(3),
        },
        ..StatusPolicy::for_market(Market::Kr)
    };
    assert_eq!(
        kr_with(NORMAL, Some("100"), &policy),
        unknown(&[SuspectReason::Stale])
    );
}

// 검증: P12
#[test]
fn 가격제한_여유를_바꾸면_판단이_따라_바뀐다() {
    let points = [(90, "127"), (5, "128")];
    let no_margin = StatusPolicy {
        near_limit_margin: d("0"),
        ..StatusPolicy::for_market(Market::Kr)
    };
    assert_eq!(kr_with(&points, Some("100"), &no_margin), normal());
    let wide = StatusPolicy {
        near_limit_margin: d("0.10"),
        ..StatusPolicy::for_market(Market::Kr)
    };
    let mid = [(90, "119"), (5, "120")];
    assert_eq!(
        kr_with(&mid, Some("100"), &wide),
        unknown(&[SuspectReason::NearUpperLimit])
    );
}

// 검증: P12
#[test]
fn 가격제한이_없으면_국내도_기준가_없이_정상이다() {
    let policy = StatusPolicy {
        limit_band: None,
        ..StatusPolicy::for_market(Market::Kr)
    };
    assert_eq!(kr_with(NORMAL, None, &policy), normal());
}

// ---------------------------------------------------------------- 합치기 · 거래 가능

// 검증: P12
#[test]
fn 공급자가_준_상태가_추정보다_앞선다() {
    for provided in [
        TradingStatus::Vi,
        TradingStatus::Halted,
        TradingStatus::Normal,
    ] {
        assert_eq!(
            resolve_status(Some(provided), &normal()),
            (provided, StatusSource::Provider)
        );
        assert_eq!(
            resolve_status(Some(provided), &unknown(&[SuspectReason::NotMoving])),
            (provided, StatusSource::Provider)
        );
    }
}

// 검증: P12
#[test]
fn 공급자가_모른다고_하면_추정이_정상이어도_모른다() {
    assert_eq!(
        resolve_status(Some(TradingStatus::Unknown), &normal()),
        (TradingStatus::Unknown, StatusSource::Provider)
    );
}

// 검증: P12
#[test]
fn 공급자가_상태를_주지_않으면_추정을_쓴다() {
    assert_eq!(
        resolve_status(None, &normal()),
        (TradingStatus::Normal, StatusSource::Estimated)
    );
    assert_eq!(
        resolve_status(None, &unknown(&[SuspectReason::Stale])),
        (TradingStatus::Unknown, StatusSource::Estimated)
    );
}

// 검증: P12
#[test]
fn 정상만_거래할_수_있다() {
    assert!(TradingStatus::Normal.is_tradable());
    assert!(!TradingStatus::Vi.is_tradable());
    assert!(!TradingStatus::Halted.is_tradable());
    assert!(!TradingStatus::Unknown.is_tradable());
}
