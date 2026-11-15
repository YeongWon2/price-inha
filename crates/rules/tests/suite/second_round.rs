//! 2차 작업(1차 검토의 참고): 시세 종목 대조 · 늦은 시세 30초 상한 · P6의 시세 반영 ·
//! 손절 · 청산의 중복 점검 · 승인된 주문의 다시 점검 · `Passed`는 `Clone`이 아님.

use std::marker::PhantomData;
use std::time::Duration as StdDuration;

use crate::common::*;
use pi_market::{Market, StalePolicy, Symbol};
use pi_paper::{InitialCash, OrderType, PaperBroker, PaperConfig, Side};
use pi_rules::{
    ApprovalGrant, BlockKind, Blocked, Outcome, Passed, PendingApproval, Principle, Purpose,
    Reason, TeamStatus, Violation, approved as approve, issue,
};

/// 막힌 까닭이 원칙 위반이어야 하고, 그 목록을 돌려준다.
fn violations(blocked: &Blocked) -> &[Violation] {
    match &blocked.reason {
        Reason::Violations(list) => list,
        other => panic!("원칙 위반이어야 한다: {other:?}"),
    }
}

/// 자동 주문 금액 한도를 10%(1,000,000원)로 낮춘 국내 장면에서 승인을 기다리는 주문을 얻는다.
/// 금액은 33 × 50,000 = 1,650,000원.
fn pending_scenario() -> (Scenario, PendingApproval) {
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("10");
    match s.check(&kr_buy()) {
        Outcome::NeedsApproval(pending) => (s, pending),
        other => panic!("승인을 기다려야 한다: {other:?}"),
    }
}

fn grant() -> ApprovalGrant {
    ApprovalGrant::new("apv-test-1", "fp-test-abcdef")
}

// 검증: P11 E11 V26
#[test]
fn a_quote_for_another_symbol_blocks_buys_and_sells() {
    let mut s = Scenario::kr();
    s.quote.symbol = Symbol::new(HYNIX);
    let blocked = expect_blocked(s.check(&kr_buy()));
    assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
    assert!(violations(&blocked).contains(&Violation::QuoteSymbolMismatch));
    assert_eq!(Violation::QuoteSymbolMismatch.principle(), Principle::P11);
    assert!(blocked.detail().contains("P11"), "{}", blocked.detail());

    // 손절도 다른 종목의 시세로는 셈하지 않는다.
    s.hold(SAMSUNG, "10", "55000", "50000");
    let blocked = expect_blocked(s.check(&kr_stop_loss()));
    assert!(violations(&blocked).contains(&Violation::QuoteSymbolMismatch));
}

// 검증: P11 N6
#[test]
fn the_stale_quote_limit_is_capped_at_thirty_seconds() {
    // 설정이 120초여도 31초 지난 시세는 늦은 것이다.
    let mut s = Scenario::kr();
    s.stale = StalePolicy {
        max_age: StdDuration::from_secs(120),
    };
    s.quote_age(31);
    let blocked = expect_blocked(s.check(&kr_buy()));
    assert!(violations(&blocked).contains(&Violation::StaleQuote));

    // 딱 30초는 아직 늦지 않다.
    s.quote_age(30);
    expect_pass(s.check(&kr_buy()));

    // 설정이 30초보다 짧으면 설정이 이긴다.
    s.stale = StalePolicy {
        max_age: StdDuration::from_secs(10),
    };
    s.quote_age(11);
    let blocked = expect_blocked(s.check(&kr_buy()));
    assert!(violations(&blocked).contains(&Violation::StaleQuote));
}

// 검증: P6 E11
#[test]
fn averaging_down_looks_at_the_lower_of_the_book_price_and_the_quote() {
    // 장부 마지막 값 51,000은 평균 단가 50,000보다 높지만 시세 49,000은 낮다 → 손실 중.
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "50000", "51000");
    s.quote.price = d("49000");
    let draft = pi_rules::OrderDraft {
        purpose: Purpose::AddOn,
        ..kr_buy()
    };
    let blocked = expect_blocked(s.check(&draft));
    assert!(violations(&blocked).contains(&Violation::AveragingDown));

    // 둘 다 평균 단가 이상이면 더 살 수 있다(같은 것은 손실이 아니다).
    s.quote.price = d("50000");
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().side, Side::Buy);
}

// 검증: P12 E34 E11
#[test]
fn stop_loss_and_liquidation_are_not_blocked_by_an_open_take_profit_sell() {
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "100", "50000", "48000");
    s.open_orders.push(approved(
        Market::Kr,
        SAMSUNG,
        Side::Sell,
        OrderType::Limit,
        "100",
        Some("56000"),
    ));
    for purpose in [Purpose::StopLoss, Purpose::Liquidation] {
        let draft = pi_rules::OrderDraft {
            purpose,
            ..kr_stop_loss()
        };
        let passed = expect_pass(s.check(&draft));
        assert_eq!(
            passed.order(),
            &approved(
                Market::Kr,
                SAMSUNG,
                Side::Sell,
                OrderType::Market,
                "100",
                None
            ),
            "{purpose:?}"
        );
    }
    // 익절끼리는 여전히 중복이다.
    let draft = pi_rules::OrderDraft {
        purpose: Purpose::TakeProfit,
        ..kr_stop_loss()
    };
    let blocked = expect_blocked(s.check(&draft));
    assert!(violations(&blocked).contains(&Violation::DuplicateOrder));
}

// 검증: E21 V25 P12
#[test]
fn a_market_buy_over_the_auto_order_limit_is_blocked_instead_of_waiting() {
    // 시장가 사기의 값 50,000 × 1.001 = 50,050, 주당 위험 3,050 → floor(100,000 / 3,050) = 32.
    // 금액 32 × 50,050 = 1,601,600 > 한도 1,000,000.
    let mut s = Scenario::kr();
    s.params.auto_order_limit_pct = d("10");
    let draft = pi_rules::OrderDraft {
        order_type: OrderType::Market,
        limit_price: None,
        ..kr_buy()
    };
    let blocked = expect_blocked(s.check(&draft));
    assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
    assert!(violations(&blocked).contains(&Violation::MarketBuyOverAutoLimit));
    assert_eq!(
        Violation::MarketBuyOverAutoLimit.principle(),
        Principle::P12
    );

    // 한도 안(20% = 2,000,000)의 시장가 사기는 그대로 지난다.
    s.params.auto_order_limit_pct = d("20");
    let passed = expect_pass(s.check(&draft));
    assert_eq!(passed.order().quantity, d("32"));
    assert_eq!(passed.amount(), d("1601600"));
}

// 검증: E21 V25 E33 C9
#[test]
fn an_approved_limit_buy_passes_after_a_fresh_check_and_gets_a_permit() {
    let (s, pending) = pending_scenario();
    let want = pending.order().clone();
    let passed = approve(pending, &grant(), &kr_buy(), &s.ctx()).unwrap();
    assert_eq!(passed.order(), &want);
    assert_eq!(passed.amount(), d("1650000"));
    assert_eq!(passed.approval(), Some(&grant()));
    assert_eq!(passed.approval().map(ApprovalGrant::id), Some("apv-test-1"));

    let config = PaperConfig::new(
        InitialCash {
            krw: d("10000000"),
            usd: d("0"),
        },
        kr_rules(),
        us_rules(),
    );
    let (_broker, authority) = PaperBroker::open(config).unwrap();
    let permit = issue(&authority, passed);
    assert_eq!(permit.approved(), &want);
}

// 검증: E21 V25 V26
#[test]
fn an_approved_buy_whose_order_changed_since_is_rejected() {
    // 시세가 49,000으로 내려 다시 셈한 값 · 수량이 승인한 내용(33주 × 50,000)과 다르다.
    let (mut s, pending) = pending_scenario();
    s.quote.price = d("49000");
    let blocked = approve(pending, &grant(), &kr_buy(), &s.ctx()).unwrap_err();
    assert_eq!(blocked.kind, BlockKind::ApprovalRejected);
    assert_eq!(blocked.reason, Reason::ApprovalMismatch);
    assert!(!blocked.detail().trim().is_empty());
}

// 검증: E21 E31 V26
#[test]
fn an_approved_buy_is_checked_again_and_blocked_when_the_team_halted_meanwhile() {
    let (mut s, pending) = pending_scenario();
    s.team = TeamStatus::DailyLimitHalt;
    let blocked = approve(pending, &grant(), &kr_buy(), &s.ctx()).unwrap_err();
    assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
    assert_eq!(
        blocked.reason,
        Reason::TeamNotOperating(TeamStatus::DailyLimitHalt)
    );
}

// 검증: E21 V25
#[test]
fn an_empty_approval_grant_does_not_turn_into_a_pass() {
    for bad in [
        ApprovalGrant::new("", "fp-test-abcdef"),
        ApprovalGrant::new("apv-test-1", "  "),
    ] {
        let (s, pending) = pending_scenario();
        let blocked = approve(pending, &bad, &kr_buy(), &s.ctx()).unwrap_err();
        assert_eq!(blocked.kind, BlockKind::ApprovalRejected, "{bad:?}");
        assert_eq!(blocked.reason, Reason::ApprovalMismatch, "{bad:?}");
    }
}

/// `T: Clone`일 때만 고르는 길(autoref 특수화). 컴파일 시험 없이 실행 중에 `Clone`인지 본다.
struct Probe<T>(PhantomData<T>);

trait CloneProbe {
    fn is_clone(&self) -> bool {
        true
    }
}
impl<T: Clone> CloneProbe for Probe<T> {}

trait NotCloneProbe {
    fn is_clone(&self) -> bool {
        false
    }
}
impl<T> NotCloneProbe for &Probe<T> {}

// 검증: E33
#[test]
fn passed_is_not_clone_so_one_check_cannot_be_issued_twice() {
    let grant_probe = &Probe::<ApprovalGrant>(PhantomData);
    let passed_probe = &Probe::<Passed>(PhantomData);
    assert!(grant_probe.is_clone(), "견본: 증표는 Clone이다");
    assert!(!passed_probe.is_clone());
}
