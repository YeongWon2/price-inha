//! 거래 상태(VI · 거래정지)와 시세로 하는 추정 (결정 2026-11-21 5번).
//!
//! 상태를 주는 공급자가 있으면 그것을 먼저 쓰고, 없으면 시세로 추정한다. 추정은 시세가 늦지
//! 않고 가격이 움직이며 가격제한에서 멀 때만 정상이고, 그 밖은 모르는 것(`Unknown`)으로 막는다.
//! 시장 규칙의 숫자는 `docs/research/stock-investing-principles.md` 5장을 따른다.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::{Market, MarketCalendar, Quote, StalePolicy, is_stale};

/// 기본 지켜보기 시간: 60초(정적 VI 2분 단일가의 뒤 절반 이상을 막는다).
const DEFAULT_STILL_AFTER: Duration = Duration::from_secs(60);

/// 거래 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TradingStatus {
    /// 정상 거래.
    Normal,
    /// 변동성 완화 장치(VI) 발동 중.
    Vi,
    /// 거래정지(미국 LULD 정지 포함).
    Halted,
    /// 모른다. 거래할 수 없는 것으로 본다.
    Unknown,
}

impl TradingStatus {
    /// 거래할 수 있는가. `Normal`만 참이다(모르면 막는다).
    pub fn is_tradable(self) -> bool {
        matches!(self, TradingStatus::Normal)
    }
}

/// 거래 상태를 어디서 얻었는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StatusSource {
    /// 공급자가 준 상태.
    Provider,
    /// 시세로 추정한 상태.
    Estimated,
}

/// 추정이 정상으로 보지 않은 까닭. 순서는 이 enum 순서다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SuspectReason {
    /// 시세가 하나도 없다.
    NoQuote,
    /// 다른 종목의 시세가 섞였다.
    MixedSymbols,
    /// `now`에 장이 닫혀 있다.
    MarketClosed,
    /// 가장 늦게 받은 시세가 늦었거나(N6), 달력이 `now`를 덮지 않는다.
    Stale,
    /// `now - still_after` 이전에 받은 시세가 없다.
    NotObservedLongEnough,
    /// `(now - still_after, now]` 안에서 가격이 한 번도 바뀌지 않았다.
    NotMoving,
    /// 가격제한이 있는 시장인데 기준가가 없다.
    NoReferencePrice,
    /// 등락률이 `+(band - margin)` 이상이다.
    NearUpperLimit,
    /// 등락률이 `-(band - margin)` 이하다.
    NearLowerLimit,
}

/// 추정 결과. `status`는 `Normal` 아니면 `Unknown`이다. `reasons`가 비었을 때만 `Normal`이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEstimate {
    /// 추정한 상태.
    pub status: TradingStatus,
    /// 의심 까닭들(enum 순서, 해당하는 것 모두).
    pub reasons: Vec<SuspectReason>,
}

/// 추정 기준(모두 설정값).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPolicy {
    /// 늦은 시세 기준(N6과 같은 기준).
    pub stale: StalePolicy,
    /// 이 시간 동안 가격이 그대로면 의심한다. 충분히 지켜봤는지도 이것으로 잰다.
    pub still_after: Duration,
    /// 가격제한 폭(예: 국내 0.30). 없으면(미국) 가격제한 검사를 건너뛴다.
    pub limit_band: Option<Decimal>,
    /// 가격제한 경계에서 이만큼 앞부터 의심한다(예: 0.02).
    pub near_limit_margin: Decimal,
}

impl StatusPolicy {
    /// 시장별 기본값: 늦음 30초, 지켜보기 60초, 국내 가격제한 0.30(미국 없음), 여유 0.02.
    pub fn for_market(market: Market) -> Self {
        let limit_band = match market {
            // 국내 가격제한 ±30%.
            Market::Kr => Some(Decimal::new(30, 2)),
            // 미국은 일일 상하한이 없다(LULD는 정지로 나타난다).
            Market::Us => None,
        };
        Self {
            stale: StalePolicy::default(),
            still_after: DEFAULT_STILL_AFTER,
            limit_band,
            near_limit_margin: Decimal::new(2, 2),
        }
    }
}

/// 한 종목의 최근 시세로 거래 상태를 추정한다(순수 함수).
///
/// `recent`의 순서는 받은 시각으로 맞춘다. `reference_price`는 가격제한의 기준가다.
/// 규칙은 `docs/dev/modules/market.md` 7차 4번을 따른다.
pub fn estimate_status(
    recent: &[Quote],
    reference_price: Option<Decimal>,
    calendar: &MarketCalendar,
    now: DateTime<Utc>,
    policy: &StatusPolicy,
) -> StatusEstimate {
    let mut sorted: Vec<&Quote> = recent.iter().collect();
    // 받은 시각으로 맞춘다. 같은 시각이면 넘긴 순서를 지킨다(안정 정렬).
    sorted.sort_by_key(|quote| quote.received_at);
    let latest = sorted.last().copied();

    let mut reasons = Vec::new();

    // 1. 시세가 있고 모두 같은 종목이다.
    match sorted.first() {
        None => reasons.push(SuspectReason::NoQuote),
        Some(first) => {
            if sorted.iter().any(|quote| quote.symbol != first.symbol) {
                reasons.push(SuspectReason::MixedSymbols);
            }
        }
    }

    // 2. 장이 열려 있다. 달력이 지금을 덮지 않으면 열렸는지 모르므로 "닫힘"이 아니라
    //    "늦음"으로 본다(N6 2차 규칙과 같다).
    let covered = calendar.covers(now);
    if covered && calendar.session_at(now).is_none() {
        reasons.push(SuspectReason::MarketClosed);
    }

    // 3. 늦지 않다: 가장 늦게 받은 시세로 잰다.
    let stale = match latest {
        Some(quote) => is_stale(quote, calendar, now, &policy.stale),
        None => !covered,
    };
    if stale {
        reasons.push(SuspectReason::Stale);
    }

    observation_reasons(&sorted, now, policy.still_after, &mut reasons);
    if let Some(band) = policy.limit_band {
        limit_reasons(
            latest,
            reference_price,
            band,
            policy.near_limit_margin,
            &mut reasons,
        );
    }

    reasons.sort();
    let status = if reasons.is_empty() {
        TradingStatus::Normal
    } else {
        TradingStatus::Unknown
    };
    StatusEstimate { status, reasons }
}

/// 4 · 5. 충분히 지켜봤고, 지켜본 동안 가격이 움직였다. `sorted`는 받은 시각 순이다.
/// 시각을 빼다 넘치면(있을 수 없는 시각) 지켜보지 않은 것으로 본다.
fn observation_reasons(
    sorted: &[&Quote],
    now: DateTime<Utc>,
    still_after: Duration,
    reasons: &mut Vec<SuspectReason>,
) {
    let window_start = chrono::Duration::from_std(still_after)
        .ok()
        .and_then(|still| now.checked_sub_signed(still));
    let observed = window_start.is_some_and(|start| {
        sorted
            .first()
            .is_some_and(|quote| quote.received_at <= start)
    });
    if !observed {
        reasons.push(SuspectReason::NotObservedLongEnough);
    }
    // `(now - still_after, now]` 안의 시세 가운데 바로 앞 시세와 가격이 다른 것이 있는가.
    let moving = window_start.is_some_and(|start| {
        sorted.windows(2).any(|pair| {
            let (before, after) = (pair[0], pair[1]);
            after.received_at > start && after.received_at <= now && after.price != before.price
        })
    });
    if !moving {
        reasons.push(SuspectReason::NotMoving);
    }
}

/// 6. 가격제한에서 멀다. 가격제한(`band`)이 있는 시장에서만 부른다.
fn limit_reasons(
    latest: Option<&Quote>,
    reference_price: Option<Decimal>,
    band: Decimal,
    margin: Decimal,
    reasons: &mut Vec<SuspectReason>,
) {
    // 기준가가 없거나 0 이하면 등락률을 잴 수 없다.
    let Some(reference) = reference_price.filter(|price| *price > Decimal::ZERO) else {
        reasons.push(SuspectReason::NoReferencePrice);
        return;
    };
    let Some(quote) = latest else {
        return;
    };
    let threshold = band - margin;
    match (quote.price - reference).checked_div(reference) {
        Some(change) if change >= threshold => reasons.push(SuspectReason::NearUpperLimit),
        Some(change) if change <= -threshold => reasons.push(SuspectReason::NearLowerLimit),
        Some(_) => {}
        // 잴 수 없으면(넘침) 가격제한 근처일 수 있다고 본다(모르면 멈춤).
        None => reasons.push(SuspectReason::NearUpperLimit),
    }
}

/// 공급자 상태와 추정을 합친다. 공급자가 주면(`Some`) 그것이 먼저다(`Unknown`도 그대로).
/// `None`이면 추정을 쓴다. 늦은 시세(N6) 판단은 이것과 따로 한다.
pub fn resolve_status(
    provided: Option<TradingStatus>,
    estimate: &StatusEstimate,
) -> (TradingStatus, StatusSource) {
    match provided {
        Some(status) => (status, StatusSource::Provider),
        None => (estimate.status, StatusSource::Estimated),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::{Currency, Session, SessionKind, Symbol, TradingDay};

    fn calendar() -> MarketCalendar {
        let day = |date: &str| {
            let date: NaiveDate = date.parse().unwrap();
            TradingDay {
                date,
                sessions: vec![Session {
                    kind: SessionKind::Regular,
                    start: date.and_hms_opt(0, 0, 0).unwrap().and_utc(),
                    end: date.and_hms_opt(6, 30, 0).unwrap().and_utc(),
                }],
            }
        };
        MarketCalendar {
            market: Market::Kr,
            previous_business_day: day("2026-03-24"),
            today: day("2026-03-25"),
            next_business_day: day("2026-03-26"),
        }
    }

    fn quote(price: i64, received_at: DateTime<Utc>) -> Quote {
        Quote {
            symbol: Symbol::new("005930"),
            price: Decimal::from(price),
            currency: Currency::Krw,
            exchange_time: None,
            received_at,
        }
    }

    #[test]
    fn 기준가가_0이면_기준가가_없는_것으로_본다() {
        let now: DateTime<Utc> = "2026-03-25T02:00:00Z".parse().unwrap();
        let recent = [
            quote(100, now - chrono::Duration::seconds(90)),
            quote(101, now - chrono::Duration::seconds(5)),
        ];
        let got = estimate_status(
            &recent,
            Some(Decimal::ZERO),
            &calendar(),
            now,
            &StatusPolicy::for_market(Market::Kr),
        );
        assert_eq!(got.reasons, vec![SuspectReason::NoReferencePrice]);
    }

    #[test]
    fn 달력이_덮지_않으면_장_닫힘이_아니라_늦음이다() {
        let now: DateTime<Utc> = "2026-03-28T02:00:00Z".parse().unwrap();
        let got = estimate_status(
            &[],
            Some(Decimal::from(100)),
            &calendar(),
            now,
            &StatusPolicy::for_market(Market::Kr),
        );
        assert!(got.reasons.contains(&SuspectReason::Stale));
        assert!(!got.reasons.contains(&SuspectReason::MarketClosed));
        assert_eq!(got.status, TradingStatus::Unknown);
    }
}
