//! 기본값과 작은 판단 함수. 공식 출처로 확인한 숫자만 기본값으로 단언한다.
//! 확인하지 못한 숫자(수수료율 · 미끄러짐 · 반올림 방향 · 미국의 팔 때 비용 · 한 시세 최대 체결
//! 수량 · 판 돈을 다시 쓰는 때)에는 기본값이 없다. 준 값이 그대로 들어가고 적용되는지만 본다.

use std::time::Duration;

use pi_market::{Currency, Market, SessionKind, StalePolicy};
use pi_paper::{
    AccountKind, BrokerError, DEFAULT_EVENT_BUFFER, InitialCash, LimitFillPrice, MarketRules,
    MaxFill, OrderGroup, OrderStatus, OrderType, PaperBroker, PaperConfig, ProceedsUsable,
    RoundingMode, SessionRules, TickBand, UnfilledTimeout, UnverifiedRules,
};

use crate::support::{Desk, SAM, at, cash, config, d, krx_ticks, market_buy, sell};

/// 확인되지 않은 숫자 한 벌(테스트 설정과 같은 값).
fn unverified() -> UnverifiedRules {
    UnverifiedRules {
        commission_rate: d("0.00015"),
        market_slippage_rate: d("0.001"),
        rounding_mode: RoundingMode::Down,
        max_fill: MaxFill::Unlimited,
        proceeds_usable: ProceedsUsable::AfterSettlement,
    }
}

/// 위와 모두 다른 값 한 벌.
fn other_unverified() -> UnverifiedRules {
    UnverifiedRules {
        commission_rate: d("0.001"),
        market_slippage_rate: d("0.01"),
        rounding_mode: RoundingMode::Up,
        max_fill: MaxFill::PerQuote(d("2")),
        proceeds_usable: ProceedsUsable::Immediately,
    }
}

fn initial() -> InitialCash {
    InitialCash {
        krw: d("10000000"),
        usd: d("10000"),
    }
}

// 검증: C10
// 출처: KRX 공식 안내 "Tick Size" 표(2026-11-14 확인), 토스 명세의 "50,000~200,000원 구간은 100원 단위".
#[test]
fn 국내_기본_호가_단위_표는_krx_표와_같다() {
    assert_eq!(MarketRules::kr(unverified()).tick_table, krx_ticks());
}

// 검증: C10
#[test]
fn 국내_호가_단위는_구간의_경계에서_바뀐다() {
    let rules = MarketRules::kr(unverified());
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
// 큰 주문: 토스 명세의 주문 생성 · 정정 요청 confirmHighValueOrder 설명("1억원 이상의 주문 시 true가
// 아니면 400 confirm-high-value-required")과 오류 예(threshold 100000000 KRW · "30억원 초과 주문 (KR)",
// maxAmount 3000000000 KRW).
#[test]
fn 국내_기본_규칙의_확인된_숫자() {
    // 확인되지 않은 값을 무엇으로 주든 확인된 기본값은 같다.
    for rules in [
        MarketRules::kr(unverified()),
        MarketRules::kr(other_unverified()),
    ] {
        assert_eq!(rules.currency, Currency::Krw);
        assert_eq!(rules.rounding.scale, 0);
        assert_eq!(rules.price_limit_rate, Some(d("0.30")));
        assert_eq!(rules.sell_tax_rate, d("0.0020"));
        assert_eq!(rules.settlement.business_days, 2);
        assert_eq!(rules.high_value_confirm_from, Some(d("100000000")));
        assert_eq!(rules.max_order_amount, Some(d("3000000000")));
    }
}

// 검증: C10
// 출처: 토스 명세의 주문 생성 요청 price 설명(1달러 미만 소수 넷째 자리, 그 이상 둘째 자리),
// 상/하한가 응답(미국은 null), 조사 문서 5.2 (결제 T+1).
// 큰 주문 기준은 명세에 원화 금액으로만 적혀 있어 미국 규칙에는 넣지 않았다(미확인).
#[test]
fn 미국_기본_규칙의_확인된_숫자() {
    let rules = MarketRules::us(unverified(), d("0.0001"));
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
    assert_eq!(rules.high_value_confirm_from, None);
    assert_eq!(rules.max_order_amount, None);
}

// 검증: C10
// 2차 5번: 최대 체결 수량과 판 돈을 쓰는 때는 이제 기본값이 아니라 분명히 받는 값이라 여기서 뺐다.
#[test]
fn 기본은_보수적인_쪽이다() {
    for rules in [
        MarketRules::kr(unverified()),
        MarketRules::us(unverified(), d("0.0001")),
    ] {
        // 정규장에서만 받는다.
        assert_eq!(rules.sessions.limit, vec![SessionKind::Regular]);
        assert_eq!(rules.sessions.market, vec![SessionKind::Regular]);
        // 지정가는 지정가로 체결한다(유리한 시세의 이득을 주지 않는다).
        assert_eq!(rules.limit_fill_price, LimitFillPrice::Limit);
    }
}

// 검증: C10
// 2차 5번: 확인되지 않은 숫자에는 기본값이 없다. 규칙을 만들 때 분명히 준 값이 그대로 들어간다
// (주지 않으면 규칙을 만들 수 없으니 계좌도 열 수 없다 — 생성자가 인자로 받는다).
#[test]
fn 확인되지_않은_숫자는_준_값이_그대로_규칙에_들어간다() {
    for given in [unverified(), other_unverified()] {
        let kr = MarketRules::kr(given.clone());
        assert_eq!(kr.commission_rate, given.commission_rate);
        assert_eq!(kr.market_slippage_rate, given.market_slippage_rate);
        assert_eq!(kr.rounding.mode, given.rounding_mode);
        assert_eq!(kr.max_fill, given.max_fill);
        assert_eq!(kr.settlement.proceeds_usable, given.proceeds_usable);

        for sell_cost in ["0.0001", "0.0000278"] {
            let us = MarketRules::us(given.clone(), d(sell_cost));
            assert_eq!(us.commission_rate, given.commission_rate);
            assert_eq!(us.market_slippage_rate, given.market_slippage_rate);
            assert_eq!(us.rounding.mode, given.rounding_mode);
            assert_eq!(us.max_fill, given.max_fill);
            assert_eq!(us.settlement.proceeds_usable, given.proceeds_usable);
            assert_eq!(us.sell_tax_rate, d(sell_cost), "미국의 팔 때 비용");
        }
    }
}

// 검증: C10
// 2차 5번: 준 숫자로 실제로 셈한다(조용히 0으로 돌지 않는다).
// 수수료 0.1% · 미끄러짐 1% · 올림 · 한 시세에 2주까지.
#[test]
fn 생성자에_준_수수료_미끄러짐_반올림_최대_체결_수량이_그대로_적용된다() {
    let config = PaperConfig::new(
        initial(),
        MarketRules::kr(other_unverified()),
        MarketRules::us(other_unverified(), d("0.0001")),
    );
    let desk = Desk::open(config);
    desk.quote(SAM, "70000", at(0));
    // 묶는 돈: 70,000 × 1.01 = 70,700. 5주 × 70,700 = 353,500, 수수료 353.5 → 올려서 354.
    let order_id = desk.accepted(&market_buy(SAM, "5"), at(1));
    assert_eq!(desk.krw(), cash("9646146", "353854", "0"));

    // 다음 시세 70,000 → 70,700에 2주만. 141,400 × 0.1% = 141.4 → 올려서 142.
    desk.quote(SAM, "70000", at(2));
    let order = desk.order(&order_id);
    assert_eq!(order.status, OrderStatus::PartialFilled);
    assert_eq!(order.execution.filled_quantity, d("2"));
    assert_eq!(order.execution.average_filled_price, Some(d("70700")));
    assert_eq!(order.execution.commission, d("142"));
}

// 검증: C10
// 2차 5번: 판 돈을 다시 쓸 수 있는 때는 기본값 없이 받는 값이다. 두 선택이 저마다 그대로 적용된다.
// 10주를 70,000에 사서 71,000에 판다. 판 돈 710,000 − 106(106.5 버림) − 1,420 = 708,474.
#[test]
fn 생성자에_준_판_돈을_쓰는_때가_그대로_적용된다() {
    let run = |proceeds_usable| {
        let rules = UnverifiedRules {
            proceeds_usable,
            ..unverified()
        };
        let config = PaperConfig::new(
            initial(),
            MarketRules::kr(rules.clone()),
            MarketRules::us(rules, d("0.0001")),
        );
        let desk = Desk::open(config);
        desk.own(SAM, "10", "70000", at(0));
        desk.accepted(&sell(SAM, "10", "71000"), at(1));
        desk.quote(SAM, "71000", at(2));
        desk.krw()
    };
    // 결제일 뒤: 결제 전인 돈 칸에 있고, 살 수 있는 돈은 그대로다.
    assert_eq!(
        run(ProceedsUsable::AfterSettlement),
        cash("9299895", "0", "708474")
    );
    // 바로: 체결되자마자 쓸 수 있는 돈이 된다.
    assert_eq!(run(ProceedsUsable::Immediately), cash("10008369", "0", "0"));
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

// 검증: N6, N1
// 2차 5번: 설정은 처음 돈과 분명히 만든 두 시장 규칙을 받는다. 늦은 시세 30초(N6)는 기본값이다.
#[test]
fn 설정은_받은_규칙을_그대로_담고_늦은_시세_30초를_쓴다() {
    let initial = InitialCash {
        krw: d("5000000"),
        usd: d("300.50"),
    };
    let kr = MarketRules::kr(unverified());
    let us = MarketRules::us(other_unverified(), d("0.0000278"));
    let config = PaperConfig::new(initial.clone(), kr.clone(), us.clone());
    assert_eq!(config.initial_cash, initial);
    assert_eq!(config.kr, kr);
    assert_eq!(config.us, us);
    assert_eq!(config.stale, StalePolicy::default());
    assert_eq!(config.stale.max_age, Duration::from_secs(30));
    assert_eq!(config.event_buffer, DEFAULT_EVENT_BUFFER);
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
    assert_eq!(desk.sellable(SAM), d("0"));
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

// 검증: N1
// 2차 12번: 구독이 쥐는 개수가 0이면 알림을 하나도 줄 수 없다. 그런 설정으로는 열지 않는다.
#[test]
fn 구독이_쥐는_개수가_0이면_계좌를_열지_않는다() {
    let mut no_buffer = config();
    no_buffer.event_buffer = 0;
    assert!(matches!(
        PaperBroker::open(no_buffer),
        Err(BrokerError::InvalidConfig(_))
    ));
}
