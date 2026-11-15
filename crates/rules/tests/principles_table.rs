//! 원칙마다 어기는 주문 안 하나씩(V24). 모두 막히고, 막힌 이유가 남는다(V26 · E10 · E11).

mod common;

use chrono::Duration;
use common::*;
use pi_market::{Currency, Symbol};
use pi_paper::{OrderType, Side};
use pi_rules::{
    BlockKind, Funding, OrderDraft, Principle, ProductKind, Purpose, RationaleField, Reason,
    Violation, check_stop_change,
};

type Mutate = fn(&mut Scenario, &mut OrderDraft);

/// 어긴 것이 무엇이어야 하는가.
enum Want {
    /// 이 위반이 들어 있다.
    Violation(Violation),
    /// 이 근거 칸들이 빠졌다.
    Missing(Vec<RationaleField>),
}

/// (이름, 어긴 원칙, 바꾸기, 기대).
fn table() -> Vec<(&'static str, Principle, Mutate, Want)> {
    vec![
        (
            "P1 한 주만 사도 1회 위험을 넘는다",
            Principle::P1,
            |s, _| s.params.risk_per_trade_pct = d("0.01"),
            Want::Violation(Violation::RiskTooLarge),
        ),
        (
            "P1 장부 통화가 시장 통화와 다르다",
            Principle::P1,
            |s, _| s.book.currency = Currency::Usd,
            Want::Violation(Violation::CurrencyMismatch),
        ),
        (
            "P2 손절값이 진입가와 같다",
            Principle::P2,
            |_, o| o.rationale.stop = Some(d("50000")),
            Want::Violation(Violation::StopNotBelowEntry),
        ),
        (
            "P2 손절값이 없다",
            Principle::P2,
            |_, o| o.rationale.stop = None,
            Want::Missing(vec![RationaleField::Stop]),
        ),
        (
            "P3 그날 손실이 한도 3%에 닿았다",
            Principle::P3,
            |s, _| s.book.equity = d("9700000"),
            Want::Violation(Violation::DailyLossLimitReached),
        ),
        (
            "P4 최고점 대비 낙폭이 한도 15%를 넘었다",
            Principle::P4,
            |s, _| s.book.peak_equity = d("12000000"),
            Want::Violation(Violation::DrawdownLimitReached),
        ),
        (
            "P5 레버리지 상품",
            Principle::P5,
            |_, o| o.product = ProductKind::Leveraged,
            Want::Violation(Violation::BannedProduct(ProductKind::Leveraged)),
        ),
        (
            "P5 인버스 상품",
            Principle::P5,
            |_, o| o.product = ProductKind::Inverse,
            Want::Violation(Violation::BannedProduct(ProductKind::Inverse)),
        ),
        (
            "P5 파생 상품",
            Principle::P5,
            |_, o| o.product = ProductKind::Derivative,
            Want::Violation(Violation::BannedProduct(ProductKind::Derivative)),
        ),
        (
            "P5 상품 종류를 모른다",
            Principle::P5,
            |_, o| o.product = ProductKind::Unknown,
            Want::Violation(Violation::BannedProduct(ProductKind::Unknown)),
        ),
        (
            "P5 미수",
            Principle::P5,
            |_, o| o.funding = Funding::Margin,
            Want::Violation(Violation::BannedFunding(Funding::Margin)),
        ),
        (
            "P5 신용",
            Principle::P5,
            |_, o| o.funding = Funding::Credit,
            Want::Violation(Violation::BannedFunding(Funding::Credit)),
        ),
        (
            "P5 설정의 금지 종목",
            Principle::P5,
            |s, _| {
                s.config
                    .banned_symbols
                    .push((pi_market::Market::Kr, Symbol::new(SAMSUNG)));
            },
            Want::Violation(Violation::BannedSymbol),
        ),
        (
            "P5 가진 것 없이 판다(공매도)",
            Principle::P5,
            |_, o| {
                o.purpose = Purpose::StopLoss;
                o.order_type = OrderType::Market;
                o.limit_price = None;
            },
            Want::Violation(Violation::ShortSale),
        ),
        (
            "P6 손실 중인 종목을 더 산다",
            Principle::P6,
            |s, o| {
                s.hold(SAMSUNG, "10", "55000", "50000");
                o.purpose = Purpose::AddOn;
            },
            Want::Violation(Violation::AveragingDown),
        ),
        (
            "P8 종목 비중 20%가 이미 찼다",
            Principle::P8,
            |s, o| {
                s.hold(SAMSUNG, "40", "45000", "50000");
                o.purpose = Purpose::AddOn;
            },
            Want::Violation(Violation::SymbolWeightFull),
        ),
        (
            "P8 업종 비중 40%가 이미 찼다",
            Principle::P8,
            |s, _| s.hold(HYNIX, "80", "40000", "50000"),
            Want::Violation(Violation::SectorWeightFull),
        ),
        (
            "P8 업종표에 없는 종목",
            Principle::P8,
            |s, _| {
                s.config.sectors.remove(&Symbol::new(SAMSUNG));
            },
            Want::Violation(Violation::UnknownSector),
        ),
        (
            "P9 가설이 없다",
            Principle::P9,
            |_, o| o.rationale.hypothesis = None,
            Want::Missing(vec![RationaleField::Hypothesis]),
        ),
        (
            "P9 계기가 공백뿐이다",
            Principle::P9,
            |_, o| o.rationale.trigger = Some("   ".to_string()),
            Want::Missing(vec![RationaleField::Trigger]),
        ),
        (
            "P9 기대값이 없다",
            Principle::P9,
            |_, o| o.rationale.expected_value = None,
            Want::Missing(vec![RationaleField::ExpectedValue]),
        ),
        (
            "P10 판단 뒤에 알게 된 자료",
            Principle::P10,
            |_, o| o.sources[0].known_at = o.decided_at + Duration::seconds(1),
            Want::Violation(Violation::DataFromFuture),
        ),
        (
            "P11 시세가 31초 늦었다",
            Principle::P11,
            |s, _| s.quote_age(31),
            Want::Violation(Violation::StaleQuote),
        ),
        (
            "P11 계좌가 어긋난다",
            Principle::P11,
            |s, _| s.health.account_in_sync = false,
            Want::Violation(Violation::AccountMismatch),
        ),
        (
            "P11 API 오류",
            Principle::P11,
            |s, _| s.health.api_error = true,
            Want::Violation(Violation::ApiError),
        ),
        (
            "P11 자료 이상",
            Principle::P11,
            |s, _| s.health.data_anomaly = true,
            Want::Violation(Violation::DataAnomaly),
        ),
        (
            "P12 장이 닫혔다",
            Principle::P12,
            |s, _| s.now = t("2026-11-16T07:00:00Z"),
            Want::Violation(Violation::MarketClosed),
        ),
        (
            "P12 지정가가 가격제한폭 밖이다",
            Principle::P12,
            |_, o| o.limit_price = Some(d("70000")),
            Want::Violation(Violation::OutsidePriceLimit),
        ),
        (
            "P12 VI가 걸렸다",
            Principle::P12,
            |s, _| s.status.vi = Some(true),
            Want::Violation(Violation::ViActive),
        ),
        (
            "P12 VI를 모른다",
            Principle::P12,
            |s, _| s.status.vi = None,
            Want::Violation(Violation::StatusUnknown),
        ),
        (
            "P12 거래정지",
            Principle::P12,
            |s, _| s.status.halted = Some(true),
            Want::Violation(Violation::TradingHalted),
        ),
        (
            "P12 거래정지를 모른다",
            Principle::P12,
            |s, _| s.status.halted = None,
            Want::Violation(Violation::StatusUnknown),
        ),
        (
            "P12 같은 종목 · 같은 방향의 주문이 이미 있다",
            Principle::P12,
            |s, _| {
                s.open_orders.push(approved(
                    pi_market::Market::Kr,
                    SAMSUNG,
                    Side::Buy,
                    OrderType::Limit,
                    "5",
                    Some("49000"),
                ));
            },
            Want::Violation(Violation::DuplicateOrder),
        ),
        (
            "P12 1회 최대 금액을 넘는다",
            Principle::P12,
            |s, _| s.rules.max_order_amount = Some(d("1000000")),
            Want::Violation(Violation::OverMaxOrderAmount),
        ),
        (
            "P12 일일 매매 횟수 5회에 닿았다",
            Principle::P12,
            |s, _| s.trades_today = 5,
            Want::Violation(Violation::TradeCountReached),
        ),
        (
            "P12 API 요청 한도에 닿았다",
            Principle::P12,
            |s, _| s.api.used = 10,
            Want::Violation(Violation::ApiLimitReached),
        ),
        (
            "P13 비용을 빼면 기대값이 0이다",
            Principle::P13,
            // 비용 = 50,000 × (2 × 0.00015 + 0.0020 + 2 × 0.001) = 215.
            |_, o| o.rationale.expected_value = Some(d("215")),
            Want::Violation(Violation::NoPositiveEdge),
        ),
    ]
}

// 검증: V24 E11 P1 P2 P3 P4 P5 P6 P8 P9 P10 P11 P12 P13
#[test]
fn every_principle_breaking_draft_is_blocked() {
    let mut covered = Vec::new();
    for (name, principle, mutate, want) in table() {
        let mut s = Scenario::kr();
        let mut draft = kr_buy();
        mutate(&mut s, &mut draft);
        let blocked = expect_blocked(s.check(&draft));
        match want {
            Want::Violation(violation) => match &blocked.reason {
                Reason::Violations(found) => {
                    assert!(found.contains(&violation), "{name}: {found:?}");
                    assert_eq!(violation.principle(), principle, "{name}");
                }
                other => panic!("{name}: 원칙 위반이어야 한다: {other:?}"),
            },
            Want::Missing(fields) => {
                assert_eq!(blocked.reason, Reason::MissingRationale(fields), "{name}");
            }
        }
        assert!(
            blocked.reason.principles().contains(&principle),
            "{name}: {:?}",
            blocked.reason.principles()
        );
        covered.push(principle);
    }
    for p in [
        Principle::P1,
        Principle::P2,
        Principle::P3,
        Principle::P4,
        Principle::P5,
        Principle::P6,
        Principle::P8,
        Principle::P9,
        Principle::P10,
        Principle::P11,
        Principle::P12,
        Principle::P13,
    ] {
        assert!(covered.contains(&p), "{p:?} 줄이 표에 없다");
    }
}

// 검증: V26 E10 E11
#[test]
fn every_block_carries_its_kind_and_a_readable_reason() {
    for (name, principle, mutate, _) in table() {
        let mut s = Scenario::kr();
        let mut draft = kr_buy();
        mutate(&mut s, &mut draft);
        let blocked = expect_blocked(s.check(&draft));
        assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale, "{name}");
        let detail = blocked.detail();
        assert!(!detail.trim().is_empty(), "{name}");
        assert!(detail.contains(principle.code()), "{name}: {detail}");
    }
}

// 검증: V26
#[test]
fn block_kinds_match_the_store_one_to_one() {
    assert_eq!(
        BlockKind::ALL,
        [
            BlockKind::PrincipleOrRationale,
            BlockKind::ApprovalRejected,
            BlockKind::UnfilledCancelled,
        ]
    );
    for kind in BlockKind::ALL {
        // 종류가 늘거나 줄면 이 match가 컴파일되지 않는다.
        let name = match kind {
            BlockKind::PrincipleOrRationale => "원칙 · 근거 부족",
            BlockKind::ApprovalRejected => "승인 거절",
            BlockKind::UnfilledCancelled => "미체결 취소",
        };
        assert!(!name.is_empty());
    }
}

// 검증: V17 P2 E10
#[test]
fn a_draft_without_a_stop_is_blocked_and_names_the_missing_stop() {
    let s = Scenario::kr();
    let mut draft = kr_buy();
    draft.rationale.stop = None;
    let blocked = expect_blocked(s.check(&draft));
    assert_eq!(
        blocked.reason,
        Reason::MissingRationale(vec![RationaleField::Stop])
    );
    assert_eq!(
        blocked.reason.principles(),
        vec![Principle::P2, Principle::P9]
    );
    assert_eq!(blocked.kind, BlockKind::PrincipleOrRationale);
}

// 검증: E10 P9
#[test]
fn all_missing_rationale_fields_are_listed_in_order() {
    let s = Scenario::kr();
    let mut draft = kr_buy();
    draft.rationale = pi_rules::Rationale {
        hypothesis: Some(String::new()),
        ..pi_rules::Rationale::default()
    };
    let blocked = expect_blocked(s.check(&draft));
    assert_eq!(
        blocked.reason,
        Reason::MissingRationale(RationaleField::ALL.to_vec())
    );
}

// 검증: E11
#[test]
fn several_broken_principles_are_all_reported() {
    let mut s = Scenario::kr();
    s.status.vi = Some(true);
    s.health.account_in_sync = false;
    let mut draft = kr_buy();
    draft.product = ProductKind::Leveraged;
    let blocked = expect_blocked(s.check(&draft));
    let principles = blocked.reason.principles();
    assert_eq!(
        principles,
        vec![Principle::P5, Principle::P11, Principle::P12]
    );
}

// 검증: E18 P12
#[test]
fn an_order_outside_market_hours_is_blocked_even_for_a_stop_loss() {
    let mut s = Scenario::kr();
    s.hold(SAMSUNG, "10", "55000", "50000");
    s.now = t("2026-11-16T07:00:00Z");
    let blocked = expect_blocked(s.check(&kr_stop_loss()));
    match blocked.reason {
        Reason::Violations(found) => assert!(found.contains(&Violation::MarketClosed)),
        other => panic!("장 밖이어야 한다: {other:?}"),
    }
}

// 검증: P2
#[test]
fn a_held_stop_may_only_be_narrowed() {
    assert_eq!(check_stop_change(d("47000"), d("48000")), Ok(()));
    assert_eq!(check_stop_change(d("47000"), d("47000")), Ok(()));
    let err = check_stop_change(d("47000"), d("46950")).unwrap_err();
    assert_eq!(err, Violation::StopWidened);
    assert_eq!(err.principle(), Principle::P2);
}

// 검증: E11
#[test]
fn principle_codes_are_the_document_numbers() {
    let all = [
        (Principle::P1, "P1"),
        (Principle::P2, "P2"),
        (Principle::P3, "P3"),
        (Principle::P4, "P4"),
        (Principle::P5, "P5"),
        (Principle::P6, "P6"),
        (Principle::P7, "P7"),
        (Principle::P8, "P8"),
        (Principle::P9, "P9"),
        (Principle::P10, "P10"),
        (Principle::P11, "P11"),
        (Principle::P12, "P12"),
        (Principle::P13, "P13"),
    ];
    for (p, code) in all {
        assert_eq!(p.code(), code);
    }
}
