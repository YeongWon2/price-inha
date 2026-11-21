//! 거래 상태(VI · 거래정지)와 시세로 하는 추정 (결정 2026-11-21 5번).
//!
//! 상태를 주는 공급자가 있으면 그것을 먼저 쓰고, 없으면 시세로 추정한다. 추정은 시세가 늦지
//! 않고 가격이 움직이며 가격제한에서 멀 때만 정상이고, 그 밖은 모르는 것(`Unknown`)으로 막는다.
//! 시장 규칙의 숫자는 `docs/research/stock-investing-principles.md` 5장을 따른다.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

use crate::{Market, MarketCalendar, Quote, StalePolicy};

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
        todo!()
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
        todo!()
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
    todo!()
}

/// 공급자 상태와 추정을 합친다. 공급자가 주면(`Some`) 그것이 먼저다(`Unknown`도 그대로).
/// `None`이면 추정을 쓴다. 늦은 시세(N6) 판단은 이것과 따로 한다.
pub fn resolve_status(
    provided: Option<TradingStatus>,
    estimate: &StatusEstimate,
) -> (TradingStatus, StatusSource) {
    todo!()
}
