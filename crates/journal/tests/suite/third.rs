//! 3차 작업(후보 일괄 처리): 연속 손실 정지 · ATR 칸의 위험 증가, 그날 손댄 매매가 뒤에 갱신돼도 하루 정리에 남기.

use pi_journal::{Day, DayFacts, Mistake, increases_risk};
use pi_store::{TradeId, TradeState};
use pi_template::{AtrBar, AtrStop, Cooldown, LossStreakStop, RiskParams};
use rust_decimal::Decimal;

use crate::common::{DAY, Env, MIN, T0, base_params, t};

/// 연속 손실 정지 칸만 바꾼 값.
fn streak(losses: u32, cooldown: Cooldown) -> RiskParams {
    let mut p = base_params();
    p.loss_streak = Some(LossStreakStop { losses, cooldown });
    p
}

/// ATR 손절 칸만 바꾼 값.
fn atr(multiple: Option<i64>) -> RiskParams {
    let mut p = base_params();
    p.stop.atr = multiple.map(|m| AtrStop {
        multiple: Decimal::new(m, 0),
        bar: AtrBar::Day,
    });
    p
}

fn check(cases: Vec<(&str, RiskParams, RiskParams, bool)>) {
    for (why, before, after, expected) in cases {
        assert_eq!(increases_risk(&before, &after), expected, "{why}");
    }
}

// 검증: M4 E28 (연속 손실 정지: 정지까지 손실 횟수를 늘리거나 쉬는 기간을 줄이면 한도를 넓힌 것)
#[test]
fn 연속_손실_정지를_느슨하게_하면_위험이_커진다() {
    let days = Cooldown::TradingDays;
    let rest = Cooldown::RestOfDay;
    check(vec![
        ("횟수 증가", streak(3, rest), streak(4, rest), true),
        ("횟수 감소", streak(4, rest), streak(3, rest), false),
        ("쉬는 날 단축", streak(3, days(2)), streak(3, days(1)), true),
        ("며칠에서 그날만", streak(3, days(1)), streak(3, rest), true),
        (
            "그날만에서 며칠",
            streak(3, rest),
            streak(3, days(1)),
            false,
        ),
        (
            "쉬는 날 연장",
            streak(3, days(1)),
            streak(3, days(2)),
            false,
        ),
        ("같음", streak(3, days(2)), streak(3, days(2)), false),
    ]);
}

// 검증: M4 E28 (ATR 손절이 사라지는 것도 손절을 넓힌 것)
#[test]
fn atr_손절이_사라지면_위험이_커진다() {
    check(vec![
        ("있음에서 없음", atr(Some(2)), atr(None), true),
        ("없음에서 없음", atr(None), atr(None), false),
    ]);
}

// 검증: V12 (그날 판 매매는 다음 날 복기돼 갱신 시각이 바뀌어도 그날 한 일에 남는다)
#[test]
fn 그날_손댄_매매는_뒤에_갱신돼도_하루_정리에_남는다() {
    let env = Env::new();
    let start = T0 + 60 * MIN;
    let sold = env.sold("005930", start + MIN);
    env.journal
        .review_trade(sold, vec![Mistake::LateStop], "복기", t(start + DAY + MIN))
        .unwrap();
    let day = Day {
        start: t(start),
        end: t(start + DAY),
    };
    let facts = DayFacts {
        realized: vec![],
        last_backup_at: None,
    };
    let summary = env.journal.daily_summary(day, &facts).unwrap();
    let ids: Vec<TradeId> = summary.done.iter().map(|line| line.id).collect();
    assert_eq!(ids, vec![sold]);
    assert_eq!(summary.done[0].state, TradeState::Reviewed);
    // 그날 복기된 것은 아니다.
    assert!(summary.reviewed.is_empty());
}
