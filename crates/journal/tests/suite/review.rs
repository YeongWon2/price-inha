//! 복기 기록 · 실수 · 못 끝낸 복기 넘기기.

use pi_journal::{Error, Mistake, ReviewRecord};
use pi_store::{StoreError, TradeId, TradeState};

use crate::common::{DAY, Env, MIN, T0, t};

// 검증: V19
#[test]
fn 팔린_매매를_복기하면_복기됨이_되고_실수_기록이_남는다() {
    let env = Env::new();
    let id = env.sold("005930", T0);
    let at = T0 + 10 * MIN;
    let record = env
        .journal
        .review_trade(
            id,
            vec![
                Mistake::ChasedEntry,
                Mistake::Other("뇌동 매매".to_string()),
            ],
            "신호보다 비싸게 샀다",
            t(at),
        )
        .unwrap();
    assert_eq!(
        record,
        ReviewRecord {
            trade: id,
            mistakes: vec![
                Mistake::ChasedEntry,
                Mistake::Other("뇌동 매매".to_string())
            ],
            note: "신호보다 비싸게 샀다".to_string(),
            at: t(at),
        }
    );
    assert_eq!(env.store.get_trade(id).unwrap().state, TradeState::Reviewed);
    assert_eq!(env.journal.reviews().unwrap(), vec![record]);
}

// 검증: V19
#[test]
fn 팔림이_아닌_매매는_복기할_수_없고_아무것도_바뀌지_않는다() {
    let env = Env::new();
    for state in [
        TradeState::Discussing,
        TradeState::Judged,
        TradeState::Ordered,
        TradeState::Holding,
    ] {
        let id = env.trade_in("005930", state, T0);
        let err = env
            .journal
            .review_trade(id, vec![Mistake::LateStop], "메모", t(T0 + MIN))
            .unwrap_err();
        assert!(
            matches!(err, Error::NotSold { id: got, state: s } if got == id && s == state),
            "{state:?}: {err:?}"
        );
        assert_eq!(env.store.get_trade(id).unwrap().state, state);
    }
    assert!(env.journal.reviews().unwrap().is_empty());
}

// 검증: V19
#[test]
fn 이미_복기한_매매를_다시_복기하면_거부되고_기록은_한_건이다() {
    let env = Env::new();
    let id = env.sold("005930", T0);
    env.journal
        .review_trade(id, vec![Mistake::PanicExit], "첫 복기", t(T0 + MIN))
        .unwrap();
    let err = env
        .journal
        .review_trade(id, vec![Mistake::LateStop], "두 번째", t(T0 + 2 * MIN))
        .unwrap_err();
    assert!(
        matches!(err, Error::NotSold { .. } | Error::Store(_)),
        "{err:?}"
    );
    let all = env.journal.reviews().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].note, "첫 복기");
}

// 검증: V19
#[test]
fn 없는_매매를_복기하면_저장소의_없음_오류다() {
    let env = Env::new();
    let err = env
        .journal
        .review_trade(TradeId(77), vec![], "메모", t(T0))
        .unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::TradeNotFound(TradeId(77)))),
        "{err:?}"
    );
    assert!(env.journal.reviews().unwrap().is_empty());
}

// 검증: V19
#[test]
fn 빈_자유_글_실수는_거부되고_매매도_기록도_그대로다() {
    let env = Env::new();
    for blank in ["", "   ", "\t\n"] {
        let id = env.sold("005930", T0);
        let err = env
            .journal
            .review_trade(
                id,
                vec![Mistake::ChasedEntry, Mistake::Other(blank.to_string())],
                "메모",
                t(T0 + MIN),
            )
            .unwrap_err();
        assert!(matches!(err, Error::BlankMistake), "{err:?}");
        assert_eq!(env.store.get_trade(id).unwrap().state, TradeState::Sold);
    }
    assert!(env.journal.reviews().unwrap().is_empty());
}

// 검증: V19
#[test]
fn 실수가_하나도_없어도_복기할_수_있다() {
    let env = Env::new();
    let id = env.sold("005930", T0);
    let record = env
        .journal
        .review_trade(id, vec![], "계획대로였다", t(T0 + MIN))
        .unwrap();
    assert!(record.mistakes.is_empty());
    assert_eq!(env.store.get_trade(id).unwrap().state, TradeState::Reviewed);
}

// 검증: V19 V20
#[test]
fn 자유_글은_앞뒤_공백을_떼고_같은_실수로_센다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    let rec = env
        .journal
        .review_trade(
            a,
            vec![Mistake::Other("  뇌동 매매 ".to_string())],
            "",
            t(T0 + MIN),
        )
        .unwrap();
    assert_eq!(rec.mistakes, vec![Mistake::Other("뇌동 매매".to_string())]);
    env.journal
        .review_trade(
            b,
            vec![Mistake::Other("뇌동 매매".to_string())],
            "",
            t(T0 + 2 * MIN),
        )
        .unwrap();
    assert_eq!(
        env.journal
            .mistake_trades(&Mistake::Other("뇌동 매매".to_string()))
            .unwrap(),
        vec![a, b]
    );
}

// 검증: V20
#[test]
fn 같은_실수는_서로_다른_매매로_세고_한_복기의_겹침은_한_번이다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    let c = env.sold("035420", T0);
    env.journal
        .review_trade(
            a,
            vec![Mistake::LateStop, Mistake::LateStop],
            "",
            t(T0 + MIN),
        )
        .unwrap();
    env.journal
        .review_trade(b, vec![Mistake::ChasedEntry], "", t(T0 + 2 * MIN))
        .unwrap();
    env.journal
        .review_trade(c, vec![Mistake::LateStop], "", t(T0 + 3 * MIN))
        .unwrap();
    assert_eq!(
        env.journal.mistake_trades(&Mistake::LateStop).unwrap(),
        vec![a, c]
    );
    assert_eq!(
        env.journal.mistake_trades(&Mistake::ChasedEntry).unwrap(),
        vec![b]
    );
    assert!(
        env.journal
            .mistake_trades(&Mistake::PanicExit)
            .unwrap()
            .is_empty()
    );
}

// 검증: E17 V19
#[test]
fn 못_끝낸_복기는_팔림으로_남아_다음_마감에_이어_복기된다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    let c = env.sold("035420", T0);
    env.journal
        .review_trade(b, vec![], "첫날 복기", t(T0 + 5 * MIN))
        .unwrap();
    let pending: Vec<TradeId> = env
        .journal
        .pending_reviews()
        .unwrap()
        .into_iter()
        .map(|trade| trade.id)
        .collect();
    assert_eq!(pending, vec![a, c]);
    for trade in env.journal.pending_reviews().unwrap() {
        assert_eq!(trade.state, TradeState::Sold);
    }

    // 다음 마감(하루 뒤) 앞에서 앞 손잡이가 닫히고 새로 열려도 이어진다.
    let env = env.reopen();
    let again: Vec<TradeId> = env
        .journal
        .pending_reviews()
        .unwrap()
        .into_iter()
        .map(|trade| trade.id)
        .collect();
    assert_eq!(again, vec![a, c]);
    env.journal
        .review_trade(
            a,
            vec![Mistake::LateStop],
            "다음 마감 복기",
            t(T0 + DAY + 5 * MIN),
        )
        .unwrap();
    env.journal
        .review_trade(c, vec![], "다음 마감 복기", t(T0 + DAY + 6 * MIN))
        .unwrap();
    assert!(env.journal.pending_reviews().unwrap().is_empty());
}

// 검증: E17
#[test]
fn 팔린_매매가_없으면_넘길_복기도_없다() {
    let env = Env::new();
    env.trade_in("005930", TradeState::Holding, T0);
    assert!(env.journal.pending_reviews().unwrap().is_empty());
}

// 검증: V19 E17
#[test]
fn 복기_기록은_다시_열어도_남은_순서대로_이어진다() {
    let env = Env::new();
    let a = env.sold("005930", T0);
    let b = env.sold("000660", T0);
    let first = env
        .journal
        .review_trade(a, vec![Mistake::NoRationale], "하나", t(T0 + MIN))
        .unwrap();
    let env = env.reopen();
    let second = env
        .journal
        .review_trade(b, vec![Mistake::OversizedPosition], "둘", t(T0 + 2 * MIN))
        .unwrap();
    let env = env.reopen();
    assert_eq!(env.journal.reviews().unwrap(), vec![first, second]);
}
