//! 이상 감지 멈춤에 들어가고 스스로 풀기(결정 2026-11-21 8번 · 13번, 상태표 · P11 · N6, 4차).
//!
//! 순수 계산이다. 신호를 모으는 것, `healthy_since`를 세는 것, 전이를 실제로 하는 것은 server다.
//! `Recovered`를 받은 쪽은 store의 `AnomalyHalt → Resting`으로 옮긴 뒤 그날 계획대로 한다.

use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{MarketCalendar, Quote, StalePolicy, Symbol};

use crate::{Health, TeamStatus};

/// 이상 감지 멈춤을 푸는 데 필요한 정상 유지 시간의 기본값: 60초(추정, 결정 13번).
/// 시세가 30초 늦음 경계를 오가며 멈춤과 풀림이 번갈아 생기지 않게 한다.
pub const DEFAULT_RECOVERY_HOLD: Duration = Duration::from_secs(60);

/// 이상 감지 멈춤을 푸는 기준(설정값).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryPolicy {
    /// 모든 것이 정상인 채로 이 시간 **이상** 지나면 푼다(딱 이 시간이면 푼다).
    pub hold: Duration,
}

impl Default for RecoveryPolicy {
    /// [`DEFAULT_RECOVERY_HOLD`](60초).
    fn default() -> Self {
        todo!()
    }
}

/// 이상 감지에 필요한 바깥 값 전부. 시각도 여기서 받는다(시계를 읽지 않는다).
#[derive(Debug, Clone)]
pub struct HealthWatch<'a> {
    /// 투자 팀 상태.
    pub team: TeamStatus,
    /// 지금 시각.
    pub now: DateTime<Utc>,
    /// 그 시장의 달력.
    pub calendar: &'a MarketCalendar,
    /// 지켜보는 종목 · 보유 종목마다 마지막 시세(없으면 `None`).
    pub quotes: &'a [(Symbol, Option<Quote>)],
    /// 늦은 시세 기준. `min(max_age, 30초)`로 다시 막는다.
    pub stale: StalePolicy,
    /// 자료와 연결의 상태.
    pub health: Health,
    /// 호출하는 쪽이 센 "모든 것이 정상이 된 첫 시각"(지금 정상이 아니면 `None`).
    pub healthy_since: Option<DateTime<Utc>>,
    /// 푸는 기준.
    pub recovery: RecoveryPolicy,
}

/// 팀 상태를 옮겨야 할 때의 전이.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthTransition {
    /// 이상 감지 멈춤으로 들어간다(`Operating` · `Resting`에서).
    AnomalyHalt,
    /// 이상이 풀렸다(`AnomalyHalt`에서). server가 `AnomalyHalt → Resting`으로 옮긴다.
    Recovered,
}

/// 지금 비정상인가: 시세 하나라도 늦음(`pi_market::is_stale`, 기준 `min(stale.max_age, 30초)`),
/// 장이 열려 있는데 시세가 `None`, 계좌 불일치, API 오류, 자료 이상.
/// 장이 닫힌 동안의 오래된 시세는 정상이고, 달력이 지금을 덮지 않으면 늦은 것이다.
pub fn is_abnormal(w: &HealthWatch<'_>) -> bool {
    let _ = w;
    todo!()
}

/// 팀 상태를 옮겨야 하는지 본다.
///
/// - 팀이 `Operating` · `Resting`이고 [`is_abnormal`]이면 [`HealthTransition::AnomalyHalt`].
///   `DailyLimitHalt` · `FullStop` · 이미 `AnomalyHalt`면 들어가지 않는다.
/// - 팀이 `AnomalyHalt`이고, 지금 비정상이 아니고, `healthy_since`가 있고 `now`보다 뒤가 아니며
///   `now - healthy_since ≥ recovery.hold`이면 [`HealthTransition::Recovered`].
/// - `FullStop`은 정상이어도 풀지 않는다(전체 정지는 사람만 푼다, E9 · V42).
/// - 그 밖은 `None`.
pub fn evaluate_health(w: &HealthWatch<'_>) -> Option<HealthTransition> {
    let _ = w;
    todo!()
}
