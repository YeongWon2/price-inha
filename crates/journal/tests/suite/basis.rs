//! 매매 근거 읽기와 근거 판정.

use pi_journal::{Error, missing_fields, plain_summary};
use pi_store::{Amount, Rationale, RationaleField, StoreError, TradeId, TradeState};

use crate::common::{Env, T0};

// 검증: C6 V18
#[test]
fn 요약에_근거_칸과_점수와_자료_시각이_쉬운_말로_들어간다() {
    let env = Env::new();
    let id = env.trade_in("005930", TradeState::Judged, T0);
    let trade = env.store.get_trade(id).unwrap();
    let text = plain_summary(&trade);
    for must in [
        "실적 발표 뒤 눌림에서 반등한다",
        "거래량 두 배 돌파",
        "분석가",
        "위험 담당",
        "이익이 늘었다",
        "변동폭이 크다",
        "분기 실적 공시",
        "2027-01-15 17:00",
    ] {
        assert!(text.contains(must), "요약에 `{must}`가 없다: {text}");
    }
    for value in ["70000", "68000", "75000", "1500"] {
        let with_comma = format!(
            "{},{}",
            &value[..value.len() - 3],
            &value[value.len() - 3..]
        );
        assert!(
            text.contains(value) || text.contains(&with_comma),
            "요약에 값 {value}이 없다: {text}"
        );
    }
    assert!(text.contains('7') && text.contains('4') && text.contains('6'));
    assert!(!text.contains("Some(") && !text.contains("None"), "{text}");
}

// 검증: C6 V18
#[test]
fn 같은_매매는_언제나_같은_글이고_다른_매매는_다르다() {
    let env = Env::new();
    let a = env.trade_in("005930", TradeState::Judged, T0);
    let b = env.trade_in("000660", TradeState::Judged, T0);
    let trade_a = env.store.get_trade(a).unwrap();
    let trade_b = env.store.get_trade(b).unwrap();
    assert_eq!(plain_summary(&trade_a), plain_summary(&trade_a));
    assert_eq!(plain_summary(&trade_a), plain_summary(&trade_a.clone()));
    assert_ne!(plain_summary(&trade_a), plain_summary(&trade_b));
}

// 검증: C6 V18 V16
#[test]
fn 판단_전_매매도_요약이_나오고_빈_칸은_개발_글자_없이_쓴다() {
    let env = Env::new();
    let id = env.trade_in("005930", TradeState::Discussing, T0);
    let trade = env.store.get_trade(id).unwrap();
    let text = plain_summary(&trade);
    assert!(!text.trim().is_empty());
    assert!(!text.contains("None") && !text.contains("Some("), "{text}");
}

// 검증: C6
#[test]
fn 매매_번호로_읽으면_저장된_매매의_요약과_같다() {
    let env = Env::new();
    let id = env.trade_in("005930", TradeState::Holding, T0);
    let expected = plain_summary(&env.store.get_trade(id).unwrap());
    assert_eq!(env.journal.read_basis(id).unwrap(), expected);
}

// 검증: C6
#[test]
fn 없는_매매를_읽으면_저장소의_없음_오류다() {
    let env = Env::new();
    let err = env.journal.read_basis(TradeId(404)).unwrap_err();
    assert!(
        matches!(err, Error::Store(StoreError::TradeNotFound(TradeId(404)))),
        "{err:?}"
    );
}

// 검증: V16
#[test]
fn 비었거나_공백뿐인_칸과_없는_칸이_모두_빈_것으로_나온다_순서는_선언_순서다() {
    let empty = Rationale::default();
    assert_eq!(
        missing_fields(&empty),
        vec![
            RationaleField::Hypothesis,
            RationaleField::Trigger,
            RationaleField::Entry,
            RationaleField::Stop,
            RationaleField::Target,
            RationaleField::ExpectedValue,
        ]
    );
    let blanks = Rationale {
        hypothesis: Some(String::new()),
        trigger: Some("   \t".to_string()),
        entry: Some(Amount(1)),
        stop: None,
        target: Some(Amount(3)),
        expected_value: Some(Amount(0)),
    };
    assert_eq!(
        missing_fields(&blanks),
        vec![
            RationaleField::Hypothesis,
            RationaleField::Trigger,
            RationaleField::Stop
        ]
    );
}

// 검증: V16
#[test]
fn 여섯_칸이_다_차면_빈_칸이_없다() {
    let full = Rationale {
        hypothesis: Some("가설".to_string()),
        trigger: Some("계기".to_string()),
        entry: Some(Amount(1)),
        stop: Some(Amount(1)),
        target: Some(Amount(1)),
        expected_value: Some(Amount(1)),
    };
    assert!(missing_fields(&full).is_empty());
}

// 검증: V16
#[test]
fn 체결된_매매는_주문됨_보유중_팔림_복기됨_모두_근거가_비어_있지_않다() {
    let env = Env::new();
    for state in [
        TradeState::Ordered,
        TradeState::Holding,
        TradeState::Sold,
        TradeState::Reviewed,
    ] {
        let id = env.trade_in("005930", state, T0);
        let trade = env.store.get_trade(id).unwrap();
        assert_eq!(trade.state, state);
        assert!(missing_fields(&trade.rationale).is_empty(), "{state:?}");
    }
}
