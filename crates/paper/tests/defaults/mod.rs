//! 기본값과 작은 판단 함수. 공식 출처로 확인한 숫자만 단언한다.
//! (수수료율 · 반올림 방향 · 미끄러짐 · 미국의 팔 때 세금은 확인하지 못해 여기서 보지 않는다.)

use std::time::Duration;

use pi_market::{Currency, Market, SessionKind, StalePolicy};
use pi_paper::{
    AccountKind, BrokerError, InitialCash, LimitFillPrice, MarketRules, MaxFill, OrderGroup,
    OrderStatus, OrderType, PaperBroker, PaperConfig, ProceedsUsable, SessionRules, TickBand,
    UnfilledTimeout,
};

use crate::support::{Desk, cash, config, d, krx_ticks};

// 검증: C10
// 출처: KRX 공식 안내 "Tick Size" 표(2026-11-14 확인), 토스 명세의 "50,000~200,000원 구간은 100원 단위".
#[test]
fn 국내_기본_호가_단위_표는_krx_표와_같다() {
    assert_eq!(MarketRules::kr().tick_table, krx_ticks());
}

// 검증: C10
#[test]
fn 국내_호가_단위는_구간의_경계에서_바뀐다() {
    let rules = MarketRules::kr();
    for (price, tick) in [
        ("1", "1"),
        ("1999", "1"),
        ("2000", "5"),
        ("4995", "5"),
        ("5000", "10"),
        ("19990", "10"),
        ("20000", "50"),
        ("49950", "50"),
        ("50000", "100"),
        ("199900", "100"),
        ("200000", "500"),
        ("499500", "500"),
        ("500000", "1000"),
        ("1234000", "1000"),
    ] {
        assert_eq!(rules.tick_for(d(price)), d(tick), "값 {price}");
    }
}

// 검증: C10
// 출처: docs/research/stock-investing-principles.md 5.1 (가격제한폭 ±30%, 증권거래세 0.20%, 결제 T+2).
#[test]
fn 국내_기본_규칙의_확인된_숫자() {
    let rules = MarketRules::kr();
    assert_eq!(rules.currency, Currency::Krw);
    assert_eq!(rules.rounding.scale, 0);
    assert_eq!(rules.price_limit_rate, Some(d("0.30")));
    assert_eq!(rules.sell_tax_rate, d("0.0020"));
    assert_eq!(rules.settlement.business_days, 2);
}

// 검증: C10
// 출처: 토스 명세의 주문 생성 요청 price 설명(1달러 미만 소수 넷째 자리, 그 이상 둘째 자리),
// 상/하한가 응답(미국은 null), 조사 문서 5.2 (결제 T+1).
#[test]
fn 미국_기본_규칙의_확인된_숫자() {
    let rules = MarketRules::us();
    assert_eq!(rules.currency, Currency::Usd);
    assert_eq!(rules.rounding.scale, 2);
    assert_eq!(
        rules.tick_table,
        vec![
            TickBand {
                below: Some(d("1")),
                tick: d("0.0001")
            },
            TickBand {
                below: None,
                tick: d("0.01")
            },
        ]
    );
    assert_eq!(rules.tick_for(d("0.9999")), d("0.0001"));
    assert_eq!(rules.tick_for(d("1")), d("0.01"));
    assert_eq!(rules.tick_for(d("150.25")), d("0.01"));
    assert_eq!(rules.price_limit_rate, None);
    assert_eq!(rules.settlement.business_days, 1);
}

// 검증: C10
#[test]
fn 기본은_보수적인_쪽이다() {
    for rules in [MarketRules::kr(), MarketRules::us()] {
        // 정규장에서만 받는다.
        assert_eq!(rules.sessions.limit, vec![SessionKind::Regular]);
        assert_eq!(rules.sessions.market, vec![SessionKind::Regular]);
        // 지정가는 지정가로 체결한다(유리한 시세의 이득을 주지 않는다).
        assert_eq!(rules.limit_fill_price, LimitFillPrice::Limit);
        assert_eq!(rules.max_fill, MaxFill::Unlimited);
        // 판 돈은 결제일 뒤에 쓴다.
        assert_eq!(
            rules.settlement.proceeds_usable,
            ProceedsUsable::AfterSettlement
        );
        // 비율은 음수가 아니다.
        assert!(rules.commission_rate >= d("0"));
        assert!(rules.market_slippage_rate >= d("0"));
    }
}

// 검증: N4
#[test]
fn 미체결_취소_기본값은_단타_1분_스윙_10분_중장기_장_마감이다() {
    assert_eq!(
        UnfilledTimeout::SCALP,
        UnfilledTimeout::After(Duration::from_secs(60))
    );
    assert_eq!(
        UnfilledTimeout::SWING,
        UnfilledTimeout::After(Duration::from_secs(600))
    );
    assert_eq!(UnfilledTimeout::LONG_TERM, UnfilledTimeout::SessionClose);
}

// 검증: N6
#[test]
fn 기본_설정은_기본_규칙과_늦은_시세_30초를_쓴다() {
    let initial = InitialCash {
        krw: d("5000000"),
        usd: d("300.50"),
    };
    let config = PaperConfig::new(initial.clone());
    assert_eq!(config.initial_cash, initial);
    assert_eq!(config.kr, MarketRules::kr());
    assert_eq!(config.us, MarketRules::us());
    assert_eq!(config.stale, StalePolicy::default());
    assert_eq!(config.stale.max_age, Duration::from_secs(30));
    assert_eq!(config.rules(Market::Kr), Some(&config.kr));
    assert_eq!(config.rules(Market::Us), Some(&config.us));
}

// 검증: C10
#[test]
fn 세션_규칙은_적힌_세션에서만_그_유형을_받는다() {
    let rules = SessionRules {
        limit: vec![SessionKind::PreMarket, SessionKind::Regular],
        market: vec![SessionKind::Regular],
    };
    assert!(rules.allows(SessionKind::PreMarket, OrderType::Limit));
    assert!(rules.allows(SessionKind::Regular, OrderType::Limit));
    assert!(rules.allows(SessionKind::Regular, OrderType::Market));
    assert!(!rules.allows(SessionKind::PreMarket, OrderType::Market));
    assert!(!rules.allows(SessionKind::AfterMarket, OrderType::Limit));
    assert!(!rules.allows(SessionKind::DayMarket, OrderType::Market));
}

// 검증: C10
// 출처: 토스 명세의 주문 목록 status 설명(OPEN 묶음 = PENDING · PARTIAL_FILLED · PENDING_CANCEL · PENDING_REPLACE).
#[test]
fn 진행_중인_상태는_명세의_open_묶음과_같다() {
    for status in [
        OrderStatus::Pending,
        OrderStatus::PartialFilled,
        OrderStatus::PendingCancel,
        OrderStatus::PendingReplace,
    ] {
        assert!(status.is_open(), "{status:?}");
    }
    for status in [
        OrderStatus::Filled,
        OrderStatus::Canceled,
        OrderStatus::Rejected,
        OrderStatus::CancelRejected,
        OrderStatus::ReplaceRejected,
        OrderStatus::Replaced,
    ] {
        assert!(!status.is_open(), "{status:?}");
    }
}

// 검증: C10, V29
#[test]
fn 새_계좌는_처음_돈만_있고_모의계좌다() {
    let desk = Desk::standard();
    assert_eq!(desk.as_broker().account_kind(), AccountKind::Paper);
    assert_eq!(desk.krw(), cash("10000000", "0", "0"));
    assert_eq!(desk.usd(), cash("10000", "0", "0"));
    assert_eq!(desk.krw().total(), d("10000000"));
    assert_eq!(desk.buying_power(Currency::Krw), d("10000000"));
    assert_eq!(desk.buying_power(Currency::Usd), d("10000"));
    assert!(desk.holdings().is_empty());
    assert!(desk.orders(OrderGroup::Open).is_empty());
    assert!(desk.orders(OrderGroup::Closed).is_empty());
    assert!(desk.broker.fills().is_empty());
    assert_eq!(desk.sellable(crate::support::SAM), d("0"));
}

// 검증: C10
// 수수료율은 확인하지 못했다. 설정한 값이 그대로 나오는지만 본다.
#[test]
fn 수수료_조회는_설정한_시장별_수수료율을_그대로_돌려준다() {
    let desk = Desk::standard();
    let commissions = desk.run(desk.as_broker().commissions()).unwrap();
    assert_eq!(commissions.len(), 2);
    assert_eq!(commissions[0].market, Market::Kr);
    assert_eq!(commissions[0].commission_rate, d("0.00015"));
    assert_eq!(commissions[1].market, Market::Us);
    assert_eq!(commissions[1].commission_rate, d("0.001"));
}

// 검증: C10
#[test]
fn 음수인_처음_돈이나_비율로는_계좌를_열지_않는다() {
    let mut negative_cash = config();
    negative_cash.initial_cash.usd = d("-1");
    assert!(matches!(
        PaperBroker::open(negative_cash),
        Err(BrokerError::InvalidConfig(_))
    ));

    let mut negative_rate = config();
    negative_rate.kr.commission_rate = d("-0.001");
    assert!(matches!(
        PaperBroker::open(negative_rate),
        Err(BrokerError::InvalidConfig(_))
    ));
}
