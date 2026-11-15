//! 수량 계산(P1 · P7 · P8), 호가 단위, 비용을 뺀 기대값(P13).

use pi_paper::MarketRules;
use rust_decimal::Decimal;

/// 수량 계산의 입력. 모두 한 통화다(통화는 섞지 않는다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeRequest {
    /// 그 통화의 평가액.
    pub equity: Decimal,
    /// 1회 위험 비율 R%(템플릿 값). 상한 2%로 다시 막는다.
    pub risk_pct: Decimal,
    /// 진입가(주문 값).
    pub entry: Decimal,
    /// 손절값.
    pub stop: Decimal,
    /// 종목당 최대 비중%(템플릿 값). 상한 25%로 다시 막는다.
    pub symbol_weight_pct: Decimal,
    /// 업종당 최대 비중%(템플릿 값). 상한 50%로 다시 막는다.
    pub sector_weight_pct: Decimal,
    /// 이미 가진 그 종목의 평가 금액.
    pub held_symbol_value: Decimal,
    /// 이미 가진 그 업종(이 종목 포함)의 평가 금액.
    pub held_sector_value: Decimal,
    /// AI가 낸 수량. 있으면 작은 쪽(P7).
    pub ai_quantity: Option<Decimal>,
}

/// 살 수량(0 이상 정수). 다음 값들 가운데 가장 작은 것이고, 0보다 작으면 0이다.
///
/// - P1: `floor(평가액 × min(R, 2) / 100 / (진입 − 손절))`. 진입 ≤ 손절이면 0.
/// - P8 종목: `floor((평가액 × min(종목%, 25) / 100 − 가진 종목 금액) / 진입)`.
/// - P8 업종: `floor((평가액 × min(업종%, 50) / 100 − 가진 업종 금액) / 진입)`.
/// - P7: `floor(AI 수량)`(있으면).
pub fn size(request: &SizeRequest) -> Decimal {
    let _ = request;
    todo!()
}

/// 값을 그 구간의 호가 단위([`MarketRules::tick_for`])로 내린다. 이미 맞으면 그대로다.
pub fn align_down_to_tick(price: Decimal, rules: &MarketRules) -> Decimal {
    let _ = (price, rules);
    todo!()
}

/// 비용을 뺀 주당 기대값(P13).
///
/// `기대값 − 진입 × (2 × 수수료율 + 매도 세율 + 2 × 미끄러짐)`. 사고팔 때 수수료와 미끄러짐을
/// 한 번씩, 팔 때 세금을 한 번 뗀다(보수적으로 파는 값도 진입가로 센다). 반올림하지 않는다.
pub fn net_expected_value(expected_value: Decimal, entry: Decimal, rules: &MarketRules) -> Decimal {
    let _ = (expected_value, entry, rules);
    todo!()
}
