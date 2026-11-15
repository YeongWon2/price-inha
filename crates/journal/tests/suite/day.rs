//! 하루 정리.

use pi_journal::{Day, DayFacts, Error, Mistake, TradeLine};
use pi_store::{Amount, TradeId, TradeState};

use crate::common::{DAY, Env, MIN, T0, t};

/// 하루의 시작: 자료를 알게 된 시각(T0)보다 한 시간 뒤다.
const S: i64 = T0 + 60 * MIN;

fn day() -> Day {
    Day {
        start: t(S),
        end: t(S + DAY),
    }
}

fn facts(realized: Vec<(TradeId, Amount)>, backup: Option<i64>) -> DayFacts {
    DayFacts {
        realized,
        last_backup_at: backup.map(t),
    }
}

// 검증: V12
#[test]
fn 하루_정리에_한_일과_손익과_복기가_한_값에_있다() {
    let env = Env::new();
    let a = env.sold("005930", S + MIN);
    let b = env.sold("000660", S + 2 * MIN);
    let c = env.trade_in("035420", TradeState::Holding, S + 3 * MIN);
    let record = env
        .journal
        .review_trade(a, vec![Mistake::LateStop], "복기", t(S + 10 * MIN))
        .unwrap();
    let summary = env
        .journal
        .daily_summary(
            day(),
            &facts(vec![(a, Amount(12_000)), (b, Amount(-5_000))], None),
        )
        .unwrap();
    assert_eq!(
        summary.done,
        vec![
            TradeLine {
                id: a,
                symbol: "005930".to_string(),
                state: TradeState::Reviewed
            },
            TradeLine {
                id: b,
                symbol: "000660".to_string(),
                state: TradeState::Sold
            },
            TradeLine {
                id: c,
                symbol: "035420".to_string(),
                state: TradeState::Holding
            },
        ]
    );
    assert_eq!(summary.pnl, Amount(7_000));
    assert_eq!(summary.reviewed, vec![record]);
}

// 검증: V12
#[test]
fn 하루_범위는_시작을_포함하고_끝을_제외한다() {
    let env = Env::new();
    let before = env.trade_in("000001", TradeState::Judged, S - 1);
    let first = env.trade_in("000002", TradeState::Judged, S);
    let last = env.trade_in("000003", TradeState::Judged, S + DAY - 1);
    let next = env.trade_in("000004", TradeState::Judged, S + DAY);
    let summary = env
        .journal
        .daily_summary(day(), &facts(vec![], None))
        .unwrap();
    let ids: Vec<TradeId> = summary.done.iter().map(|line| line.id).collect();
    assert_eq!(ids, vec![first, last]);
    assert!(!ids.contains(&before) && !ids.contains(&next));
}

// 검증: V12
#[test]
fn 손익은_음수_합도_맞고_매매가_없으면_영이다() {
    let env = Env::new();
    let a = env.sold("005930", S + MIN);
    let empty = env
        .journal
        .daily_summary(day(), &facts(vec![], None))
        .unwrap();
    assert_eq!(empty.pnl, Amount(0));
    let loss = env
        .journal
        .daily_summary(
            day(),
            &facts(vec![(a, Amount(-300)), (a, Amount(-200))], None),
        )
        .unwrap();
    assert_eq!(loss.pnl, Amount(-500));
}

// 검증: V12
#[test]
fn 손익_합이_정수_범위를_넘으면_거부한다() {
    let env = Env::new();
    let a = env.sold("005930", S + MIN);
    let err = env
        .journal
        .daily_summary(
            day(),
            &facts(vec![(a, Amount(i64::MAX)), (a, Amount(1))], None),
        )
        .unwrap_err();
    assert!(matches!(err, Error::Overflow), "{err:?}");
    let err = env
        .journal
        .daily_summary(
            day(),
            &facts(vec![(a, Amount(i64::MIN)), (a, Amount(-1))], None),
        )
        .unwrap_err();
    assert!(matches!(err, Error::Overflow), "{err:?}");
}

// 검증: V12
#[test]
fn 하루_끝이_시작보다_늦지_않으면_거부한다() {
    let env = Env::new();
    for (start, end) in [(S, S), (S + 1, S)] {
        let err = env
            .journal
            .daily_summary(
                Day {
                    start: t(start),
                    end: t(end),
                },
                &facts(vec![], None),
            )
            .unwrap_err();
        assert!(matches!(err, Error::BadDay), "{err:?}");
    }
}

// 검증: V12 E17
#[test]
fn 복기가_남았으면_하루_정리에_남았다고_보이고_다음_날에도_이어진다() {
    let env = Env::new();
    let a = env.sold("005930", S + MIN);
    let b = env.sold("000660", S + 2 * MIN);
    env.journal
        .review_trade(a, vec![], "", t(S + 10 * MIN))
        .unwrap();
    let summary = env
        .journal
        .daily_summary(day(), &facts(vec![], None))
        .unwrap();
    assert_eq!(summary.unreviewed, vec![b]);

    let env = env.reopen();
    let next_day = Day {
        start: t(S + DAY),
        end: t(S + 2 * DAY),
    };
    let summary = env
        .journal
        .daily_summary(next_day, &facts(vec![], None))
        .unwrap();
    assert_eq!(summary.unreviewed, vec![b]);
    assert!(summary.reviewed.is_empty());
    env.journal
        .review_trade(b, vec![], "", t(S + DAY + MIN))
        .unwrap();
    let summary = env
        .journal
        .daily_summary(next_day, &facts(vec![], None))
        .unwrap();
    assert!(summary.unreviewed.is_empty());
    assert_eq!(summary.reviewed.len(), 1);
    assert_eq!(summary.reviewed[0].trade, b);
}

// 검증: V50
#[test]
fn 하루_정리에_마지막_백업_완료_시각이_그대로_보인다() {
    let env = Env::new();
    let none = env
        .journal
        .daily_summary(day(), &facts(vec![], None))
        .unwrap();
    assert_eq!(none.last_backup_at, None);
    let done = env
        .journal
        .daily_summary(day(), &facts(vec![], Some(S + 20 * MIN)))
        .unwrap();
    assert_eq!(done.last_backup_at, Some(t(S + 20 * MIN)));
}

// 검증: V12 V50
#[test]
fn 다시_열어도_같은_하루_정리가_나온다() {
    let env = Env::new();
    let a = env.sold("005930", S + MIN);
    env.journal
        .review_trade(a, vec![Mistake::PanicExit], "복기", t(S + 5 * MIN))
        .unwrap();
    let f = facts(vec![(a, Amount(900))], Some(S + 30 * MIN));
    let before = env.journal.daily_summary(day(), &f).unwrap();
    let env = env.reopen();
    let after = env.journal.daily_summary(day(), &f).unwrap();
    assert_eq!(before, after);
}
