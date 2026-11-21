//! 2. 이상 감지 멈춤에 들어가고 스스로 풀기(결정 2026-11-21 8번 · 13번 · 상태표 · P11 · N6).

use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use pi_market::{MarketCalendar, Quote, StalePolicy, Symbol};
use pi_rules::{
    DEFAULT_RECOVERY_HOLD, Health, HealthTransition, HealthWatch, RecoveryPolicy, TeamStatus,
    evaluate_health, is_abnormal,
};

use crate::common::{HYNIX, KR_NOW, SAMSUNG, d, healthy, kr_fake, t};

/// 이상 감지 한 번에 필요한 값. 기본: 국내 장중 · 삼성전자 · 하이닉스 시세 모두 5초 전 · 정상.
struct Watch {
    now: DateTime<Utc>,
    calendar: MarketCalendar,
    quotes: Vec<(Symbol, Option<Quote>)>,
    stale: StalePolicy,
    health: Health,
}

impl Watch {
    fn at(now: DateTime<Utc>) -> Self {
        let (samsung, calendar) = kr_fake(SAMSUNG, d("50000"), now - Duration::seconds(5));
        let (hynix, _) = kr_fake(HYNIX, d("120000"), now - Duration::seconds(5));
        Self {
            now,
            calendar,
            quotes: vec![
                (Symbol::new(SAMSUNG), Some(samsung)),
                (Symbol::new(HYNIX), Some(hynix)),
            ],
            stale: StalePolicy::default(),
            health: healthy(),
        }
    }

    fn kr() -> Self {
        Self::at(t(KR_NOW))
    }

    /// 하이닉스 시세를 받은 시각을 지금에서 `seconds`초 전으로 바꾼다.
    fn hynix_age(&mut self, seconds: i64) {
        if let Some(quote) = self.quotes[1].1.as_mut() {
            quote.received_at = self.now - Duration::seconds(seconds);
        }
    }

    fn view(
        &self,
        team: TeamStatus,
        healthy_since: Option<DateTime<Utc>>,
        recovery: RecoveryPolicy,
    ) -> HealthWatch<'_> {
        HealthWatch {
            team,
            now: self.now,
            calendar: &self.calendar,
            quotes: &self.quotes,
            stale: self.stale,
            health: self.health,
            healthy_since,
            recovery,
        }
    }

    fn halted_since(&self, seconds: i64) -> Option<HealthTransition> {
        evaluate_health(&self.view(
            TeamStatus::AnomalyHalt,
            Some(self.now - Duration::seconds(seconds)),
            RecoveryPolicy::default(),
        ))
    }

    fn abnormal(&self) -> bool {
        is_abnormal(&self.view(TeamStatus::Operating, None, RecoveryPolicy::default()))
    }
}

/// 이상 하나씩 넣은 장면들: (이름, 비정상인가).
fn conditions() -> Vec<(&'static str, Watch, bool)> {
    let normal = Watch::kr();
    let mut late = Watch::kr();
    late.hynix_age(31);
    let mut missing = Watch::kr();
    missing.quotes[1].1 = None;
    let mut account = Watch::kr();
    account.health.account_in_sync = false;
    let mut api = Watch::kr();
    api.health.api_error = true;
    let mut data = Watch::kr();
    data.health.data_anomaly = true;
    vec![
        ("정상", normal, false),
        ("늦은 시세", late, true),
        ("시세 없음", missing, true),
        ("계좌 불일치", account, true),
        ("API 오류", api, true),
        ("자료 이상", data, true),
    ]
}

// 검증: N6 P11 E23 E24
#[test]
fn only_operating_and_resting_teams_enter_the_anomaly_halt() {
    for (name, watch, abnormal) in conditions() {
        assert_eq!(watch.abnormal(), abnormal, "{name}");
        for team in TeamStatus::ALL {
            let want = if abnormal && matches!(team, TeamStatus::Operating | TeamStatus::Resting) {
                Some(HealthTransition::AnomalyHalt)
            } else {
                None
            };
            let got = evaluate_health(&watch.view(team, None, RecoveryPolicy::default()));
            assert_eq!(got, want, "{name} {team:?}");
        }
    }
}

// 검증: N6 P11
#[test]
fn a_quote_exactly_thirty_seconds_old_is_still_fresh() {
    let mut w = Watch::kr();
    w.hynix_age(30);
    assert!(!w.abnormal());
    w.hynix_age(31);
    assert!(w.abnormal());
}

// 검증: N6 P11
#[test]
fn the_stale_limit_is_capped_at_thirty_seconds_even_if_set_longer() {
    let mut w = Watch::kr();
    w.stale = StalePolicy {
        max_age: StdDuration::from_secs(60),
    };
    w.hynix_age(31);
    assert!(w.abnormal());
    assert_eq!(
        evaluate_health(&w.view(TeamStatus::Operating, None, RecoveryPolicy::default())),
        Some(HealthTransition::AnomalyHalt)
    );
}

// 검증: N6 P11 E18
#[test]
fn old_quotes_while_the_market_is_closed_are_normal_but_an_outdated_calendar_is_not() {
    // 장 마감(06:30Z) 뒤 17:00 KST: 두 시간 전 시세도, 시세 없음도 정상이다.
    let mut closed = Watch::at(t("2026-11-16T08:00:00Z"));
    closed.hynix_age(7200);
    assert!(!closed.abnormal());
    closed.quotes[1].1 = None;
    assert!(!closed.abnormal());

    // 달력이 덮지 않는 날(11-25)은 장이 열렸는지 모르니 늦은 것으로 본다.
    let outside = Watch::at(t("2026-11-25T01:00:00Z"));
    assert!(outside.abnormal());
    assert_eq!(
        evaluate_health(&outside.view(TeamStatus::Resting, None, RecoveryPolicy::default())),
        Some(HealthTransition::AnomalyHalt)
    );
}

// 검증: N6 P11
#[test]
fn recovery_needs_sixty_healthy_seconds() {
    let w = Watch::kr();
    assert_eq!(w.halted_since(59), None);
    assert_eq!(w.halted_since(60), Some(HealthTransition::Recovered));
    assert_eq!(w.halted_since(600), Some(HealthTransition::Recovered));
}

// 검증: N6 P11
#[test]
fn recovery_does_not_happen_while_abnormal_or_without_a_healthy_start() {
    // 지금 비정상이면 healthy_since가 있어도 풀지 않는다.
    let mut late = Watch::kr();
    late.hynix_age(31);
    assert_eq!(late.halted_since(600), None);
    let mut api = Watch::kr();
    api.health.api_error = true;
    assert_eq!(api.halted_since(600), None);

    // healthy_since가 없으면 풀지 않는다.
    let w = Watch::kr();
    assert_eq!(
        evaluate_health(&w.view(TeamStatus::AnomalyHalt, None, RecoveryPolicy::default())),
        None
    );
    // healthy_since가 지금보다 뒤면(시계 어긋남) 풀지 않는다.
    assert_eq!(w.halted_since(-120), None);
}

// 검증: N6 P11
#[test]
fn a_configured_hold_is_respected() {
    let w = Watch::kr();
    let policy = RecoveryPolicy {
        hold: StdDuration::from_secs(120),
    };
    let since = |seconds: i64| Some(w.now - Duration::seconds(seconds));
    assert_eq!(
        evaluate_health(&w.view(TeamStatus::AnomalyHalt, since(119), policy)),
        None
    );
    assert_eq!(
        evaluate_health(&w.view(TeamStatus::AnomalyHalt, since(120), policy)),
        Some(HealthTransition::Recovered)
    );
}

// 검증: N6 P11 E24 P4
#[test]
fn only_the_anomaly_halt_recovers_and_full_stop_never_does() {
    let w = Watch::kr();
    for team in [
        TeamStatus::Operating,
        TeamStatus::Resting,
        TeamStatus::DailyLimitHalt,
        TeamStatus::FullStop,
    ] {
        let got = evaluate_health(&w.view(
            team,
            Some(w.now - Duration::seconds(3600)),
            RecoveryPolicy::default(),
        ));
        assert_eq!(got, None, "{team:?}");
    }
}

// 검증: N6 P11
#[test]
fn the_default_recovery_hold_is_sixty_seconds() {
    assert_eq!(DEFAULT_RECOVERY_HOLD, StdDuration::from_secs(60));
    assert_eq!(RecoveryPolicy::default().hold, StdDuration::from_secs(60));
}
