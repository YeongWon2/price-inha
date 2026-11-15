//! 손실과 멈춤(T13 · E23 · E24 · P3 · P4, 결정 31번).

mod common;

use common::*;
use pi_market::Currency;
use pi_rules::{
    Book, StopContext, StopTransition, TeamStatus, day_loss_pct, drawdown_pct, evaluate_stops,
};

fn book(currency: Currency, start: &str, now: &str, peak: &str) -> Book {
    Book {
        currency,
        equity: d(now),
        day_start_equity: d(start),
        peak_equity: d(peak),
        positions: Vec::new(),
    }
}

fn krw(start: &str, now: &str, peak: &str) -> Book {
    book(Currency::Krw, start, now, peak)
}

fn usd(start: &str, now: &str, peak: &str) -> Book {
    book(Currency::Usd, start, now, peak)
}

fn evaluate(
    team: TeamStatus,
    params: &pi_template::RiskParams,
    books: &[Book],
) -> Option<StopTransition> {
    evaluate_stops(&StopContext {
        team,
        params,
        books,
    })
}

// 검증: P3 E23
#[test]
fn day_loss_is_measured_from_the_day_start_equity() {
    assert_eq!(
        day_loss_pct(&krw("10000000", "9700000", "10000000")),
        d("3")
    );
    assert_eq!(
        day_loss_pct(&krw("10000000", "9876543", "10000000")),
        d("1.23457")
    );
    // 오르면 손실은 0이다.
    assert_eq!(
        day_loss_pct(&krw("10000000", "10500000", "10500000")),
        d("0")
    );
    assert_eq!(
        day_loss_pct(&usd("2000.00", "1950.00", "2100.00")),
        d("2.5")
    );
}

// 검증: P4 E24
#[test]
fn drawdown_is_measured_from_the_peak_equity() {
    assert_eq!(
        drawdown_pct(&krw("10000000", "10200000", "12000000")),
        d("15")
    );
    assert_eq!(
        drawdown_pct(&krw("10000000", "12000000", "12000000")),
        d("0")
    );
    assert_eq!(
        drawdown_pct(&usd("2100.00", "1950.00", "2000.00")),
        d("2.5")
    );
}

// 검증: E23 P3
#[test]
fn reaching_the_daily_loss_limit_asks_for_the_daily_halt() {
    let p = params(); // 하루 3% · 최대 낙폭 15%
    let at_limit = [krw("10000000", "9700000", "10000000")];
    assert_eq!(
        evaluate(TeamStatus::Operating, &p, &at_limit),
        Some(StopTransition::DailyLimitHalt)
    );
    let just_under = [krw("10000000", "9700001", "10000000")];
    assert_eq!(evaluate(TeamStatus::Operating, &p, &just_under), None);
}

// 검증: E24 P4
#[test]
fn reaching_the_max_drawdown_asks_for_the_full_stop() {
    let p = params();
    let at_limit = [krw("10200000", "10200000", "12000000")];
    assert_eq!(
        evaluate(TeamStatus::Operating, &p, &at_limit),
        Some(StopTransition::FullStop)
    );
    let just_under = [krw("10200001", "10200001", "12000000")];
    assert_eq!(evaluate(TeamStatus::Operating, &p, &just_under), None);
}

// 검증: E23 E24 P3 P4
#[test]
fn the_full_stop_wins_when_both_limits_are_reached() {
    let p = params();
    let both = [krw("11000000", "10000000", "12000000")];
    assert_eq!(
        evaluate(TeamStatus::Operating, &p, &both),
        Some(StopTransition::FullStop)
    );
    assert_eq!(
        evaluate(TeamStatus::DailyLimitHalt, &p, &both),
        Some(StopTransition::FullStop)
    );
}

// 검증: E23 E24
#[test]
fn no_transition_is_asked_for_a_state_already_reached() {
    let p = params();
    let day = [krw("10000000", "9700000", "10000000")];
    assert_eq!(evaluate(TeamStatus::DailyLimitHalt, &p, &day), None);
    assert_eq!(evaluate(TeamStatus::FullStop, &p, &day), None);
    let deep = [krw("10000000", "9000000", "12000000")];
    assert_eq!(evaluate(TeamStatus::FullStop, &p, &deep), None);
}

// 검증: E23 E24
#[test]
fn halts_are_asked_from_every_other_state() {
    let p = params();
    let day = [krw("10000000", "9700000", "10000000")];
    let deep = [krw("10200000", "10200000", "12000000")];
    for team in [
        TeamStatus::Operating,
        TeamStatus::Resting,
        TeamStatus::AnomalyHalt,
    ] {
        assert_eq!(
            evaluate(team, &p, &day),
            Some(StopTransition::DailyLimitHalt),
            "{team:?}"
        );
        assert_eq!(
            evaluate(team, &p, &deep),
            Some(StopTransition::FullStop),
            "{team:?}"
        );
    }
}

// 검증: P3 P4
#[test]
fn limits_over_the_absolute_caps_are_capped_again() {
    let mut p = params();
    p.daily_loss_limit_pct = d("8");
    p.max_drawdown_pct = d("40");
    // 하루 5%(상한)에 닿는다.
    let day = [krw("10000000", "9500000", "10000000")];
    assert_eq!(
        evaluate(TeamStatus::Operating, &p, &day),
        Some(StopTransition::DailyLimitHalt)
    );
    let day_under = [krw("10000000", "9500001", "10000000")];
    assert_eq!(evaluate(TeamStatus::Operating, &p, &day_under), None);
    // 낙폭 20%(상한)에 닿는다.
    let deep = [krw("10000000", "10000000", "12500000")];
    assert_eq!(
        evaluate(TeamStatus::Operating, &p, &deep),
        Some(StopTransition::FullStop)
    );
}

// 검증: P3 E23
#[test]
fn currencies_are_counted_separately_never_summed() {
    let p = params();
    // 원화 2% · 달러 2%: 어느 쪽도 3%에 닿지 않는다(더하지 않는다).
    let neither = [
        krw("10000000", "9800000", "10000000"),
        usd("2000.00", "1960.00", "2000.00"),
    ];
    assert_eq!(evaluate(TeamStatus::Operating, &p, &neither), None);
    // 달러만 3%에 닿는다.
    let dollar = [
        krw("10000000", "10100000", "10100000"),
        usd("2000.00", "1940.00", "2000.00"),
    ];
    assert_eq!(
        evaluate(TeamStatus::Operating, &p, &dollar),
        Some(StopTransition::DailyLimitHalt)
    );
}

// 검증: E24 P4
#[test]
fn a_drawdown_in_one_currency_is_enough_for_the_full_stop() {
    let p = params();
    let books = [
        krw("10000000", "10000000", "10000000"),
        usd("1700.00", "1700.00", "2000.00"),
    ];
    assert_eq!(
        evaluate(TeamStatus::Operating, &p, &books),
        Some(StopTransition::FullStop)
    );
}
