//! 수량 계산(P1 · P7 · P8), 호가 단위, 비용을 뺀 기대값(P13).

mod common;

use common::*;
use pi_rules::{SizeRequest, align_down_to_tick, net_expected_value, size};
use rust_decimal::Decimal;

fn request(equity: &str, risk: &str, entry: &str, stop: &str) -> SizeRequest {
    SizeRequest {
        equity: d(equity),
        risk_pct: d(risk),
        entry: d(entry),
        stop: d(stop),
        symbol_weight_pct: d("20"),
        sector_weight_pct: d("40"),
        held_symbol_value: Decimal::ZERO,
        held_sector_value: Decimal::ZERO,
        ai_quantity: None,
    }
}

// 검증: P1 P7 P8
#[test]
fn quantities_match_hand_computed_values() {
    let base = || request("10000000", "1", "50000", "47000");
    let cases: Vec<(&str, SizeRequest, &str)> = vec![
        // 100,000 / 3,000 = 33.33 → 33 (종목 40 · 업종 80).
        ("기본", base(), "33"),
        (
            "AI 수량이 더 작다",
            SizeRequest {
                ai_quantity: Some(d("10")),
                ..base()
            },
            "10",
        ),
        (
            "AI 수량이 더 크다",
            SizeRequest {
                ai_quantity: Some(d("500")),
                ..base()
            },
            "33",
        ),
        (
            "AI 수량의 소수는 버린다",
            SizeRequest {
                ai_quantity: Some(d("12.9")),
                ..base()
            },
            "12",
        ),
        (
            // 남은 종목 몫 2,000,000 − 1,500,000 = 500,000 → 10.
            "종목 비중이 거의 찼다",
            SizeRequest {
                held_symbol_value: d("1500000"),
                held_sector_value: d("1500000"),
                ..base()
            },
            "10",
        ),
        (
            // 남은 업종 몫 4,000,000 − 3,800,000 = 200,000 → 4.
            "업종 비중이 거의 찼다",
            SizeRequest {
                held_sector_value: d("3800000"),
                ..base()
            },
            "4",
        ),
        (
            "종목 비중이 이미 넘쳤다",
            SizeRequest {
                held_symbol_value: d("2500000"),
                held_sector_value: d("2500000"),
                ..base()
            },
            "0",
        ),
        (
            "손절이 진입과 같다",
            request("10000000", "1", "50000", "50000"),
            "0",
        ),
        (
            "손절이 진입보다 높다",
            request("10000000", "1", "50000", "51000"),
            "0",
        ),
        // R 5% → 상한 2%: 200,000 / 10,000 = 20 (종목 40 · 업종 80).
        (
            "R이 상한을 넘는다",
            request("10000000", "5", "50000", "40000"),
            "20",
        ),
        (
            // R 2%: 200,000 / 1,000 = 200. 종목 30% → 25%: 50. 업종 60% → 50%: 100.
            "비중이 상한을 넘는다",
            SizeRequest {
                symbol_weight_pct: d("30"),
                sector_weight_pct: d("60"),
                ..request("10000000", "2", "50000", "49000")
            },
            "50",
        ),
        (
            // 업종 60% → 50%: 5,000,000 − 4,900,000 = 100,000 → 2.
            "업종 비중 상한을 넘는 템플릿 값",
            SizeRequest {
                symbol_weight_pct: d("25"),
                sector_weight_pct: d("60"),
                held_sector_value: d("4900000"),
                ..request("10000000", "2", "50000", "49000")
            },
            "2",
        ),
        // 달러: 100 / 1.50 = 66.67 → 66 (종목 2,000 / 25.50 = 78).
        ("달러", request("10000.00", "1", "25.50", "24.00"), "66"),
        // 위험 금액 100,000 / 주당 위험 200,000 → 0.
        (
            "한 주도 못 산다",
            request("10000000", "1", "1000000", "800000"),
            "0",
        ),
    ];
    for (name, req, want) in cases {
        assert_eq!(size(&req), d(want), "{name}");
    }
}

/// 결정적인 의사 난수(속성 테스트용, proptest는 허용 목록에 없다).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn between(&mut self, low: i64, high: i64) -> i64 {
        low + (self.next() % (high - low + 1) as u64) as i64
    }
}

// 검증: P1 P8
#[test]
fn for_any_input_the_risk_and_the_weights_stay_under_the_caps() {
    let hundred = Decimal::from(100);
    let mut rng = Lcg(20_261_116);
    for _ in 0..5000 {
        let equity = Decimal::from(rng.between(100_000, 2_000_000_000));
        let entry = Decimal::new(rng.between(100, 50_000_000), 2);
        let stop = entry * Decimal::new(rng.between(0, 1200), 3);
        let risk_pct = Decimal::new(rng.between(1, 600), 2);
        let symbol_pct = Decimal::new(rng.between(0, 6000), 2);
        let sector_pct = Decimal::new(rng.between(0, 9000), 2);
        let held_symbol = equity * Decimal::new(rng.between(0, 400), 3) / hundred;
        let held_sector = held_symbol + equity * Decimal::new(rng.between(0, 400), 3) / hundred;
        let ai = match rng.between(0, 2) {
            0 => None,
            _ => Some(Decimal::new(rng.between(0, 1_000_000), 1)),
        };
        let req = SizeRequest {
            equity,
            risk_pct,
            entry,
            stop,
            symbol_weight_pct: symbol_pct,
            sector_weight_pct: sector_pct,
            held_symbol_value: held_symbol,
            held_sector_value: held_sector,
            ai_quantity: ai,
        };
        let q = size(&req);
        let ctx = format!("{req:?} → {q}");
        assert!(q >= Decimal::ZERO, "{ctx}");
        assert_eq!(q, q.trunc(), "정수가 아니다: {ctx}");
        if let Some(ai) = ai {
            assert!(q <= ai.floor(), "AI 수량보다 크다: {ctx}");
        }
        if q > Decimal::ZERO {
            assert!(entry > stop, "{ctx}");
            assert!(
                q * (entry - stop) <= equity * risk_pct.min(d("2")) / hundred,
                "1회 위험 초과: {ctx}"
            );
            assert!(
                held_symbol + q * entry <= equity * symbol_pct.min(d("25")) / hundred,
                "종목 비중 초과: {ctx}"
            );
            assert!(
                held_sector + q * entry <= equity * sector_pct.min(d("50")) / hundred,
                "업종 비중 초과: {ctx}"
            );
        }
    }
}

// 검증: P1
#[test]
fn the_risk_bound_is_tight_one_more_share_would_break_it() {
    let hundred = Decimal::from(100);
    let mut rng = Lcg(7);
    for _ in 0..2000 {
        let equity = Decimal::from(rng.between(1_000_000, 1_000_000_000));
        let entry = Decimal::from(rng.between(1_000, 500_000));
        let stop = entry - Decimal::from(rng.between(1, 999));
        let req = SizeRequest {
            equity,
            risk_pct: d("1"),
            entry,
            stop,
            symbol_weight_pct: d("25"),
            sector_weight_pct: d("50"),
            held_symbol_value: Decimal::ZERO,
            held_sector_value: Decimal::ZERO,
            ai_quantity: None,
        };
        let q = size(&req);
        let by_risk = (equity / hundred / (entry - stop)).floor();
        let by_symbol = (equity * d("25") / hundred / entry).floor();
        assert_eq!(q, by_risk.min(by_symbol), "{req:?}");
    }
}

// 검증: P7 P12
#[test]
fn prices_are_moved_down_onto_the_tick() {
    let kr = kr_rules();
    let us = us_rules();
    let cases = [
        (&kr, "49975", "49950"),
        (&kr, "50000", "50000"),
        (&kr, "2003", "2000"),
        (&kr, "1999", "1999"),
        (&kr, "199999", "199900"),
        (&kr, "200400", "200000"),
        (&kr, "512345", "512000"),
        (&us, "12.345", "12.34"),
        (&us, "25.50", "25.50"),
        (&us, "0.12345", "0.1234"),
    ];
    for (rules, price, want) in cases {
        assert_eq!(align_down_to_tick(d(price), rules), d(want), "{price}");
    }
}

// 검증: P13
#[test]
fn net_expected_value_subtracts_round_trip_costs() {
    // 국내: 50,000 × (2 × 0.00015 + 0.0020 + 2 × 0.001) = 50,000 × 0.0043 = 215.
    assert_eq!(
        net_expected_value(d("1000"), d("50000"), &kr_rules()),
        d("785")
    );
    assert_eq!(
        net_expected_value(d("215"), d("50000"), &kr_rules()),
        d("0")
    );
    assert_eq!(
        net_expected_value(d("100"), d("50000"), &kr_rules()),
        d("-115")
    );
    // 미국(팔 때 비용 0): 25.50 × (0.0003 + 0 + 0.002) = 0.05865.
    assert_eq!(
        net_expected_value(d("1.00"), d("25.50"), &us_rules()),
        d("0.94135")
    );
}
