//! 점검 결과와 막힌 이유(C9 · E10 · E11 · E20 · E21 · E31 · V26).

use pi_paper::ApprovedOrder;
use rust_decimal::Decimal;

use crate::{ApprovalGrant, Funding, ProductKind, RationaleField, TeamStatus, TradeStage};

/// 규칙 엔진이 맡은 원칙 번호(`principles.md` P1~P13, 결정 2026-11-16 3번).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Principle {
    /// 1회 최대 손실.
    P1,
    /// 손절 필수, 보유 중 손절은 좁히기만.
    P2,
    /// 일일 손실 한도.
    P3,
    /// 최대 낙폭.
    P4,
    /// 금지 상품.
    P5,
    /// 물타기 금지.
    P6,
    /// 수량 · 가격은 규칙 엔진이 계산.
    P7,
    /// 종목 · 업종 비중 상한.
    P8,
    /// 근거 기록.
    P9,
    /// 판단 시점까지 알 수 있던 자료만.
    P10,
    /// 이상이면 신규 주문 정지.
    P11,
    /// 주문 직전 점검.
    P12,
    /// 비용을 뺀 기대값이 양수.
    P13,
}

impl Principle {
    /// 화면과 기록에 쓰는 번호 글자: `"P1"` ~ `"P13"`.
    pub fn code(self) -> &'static str {
        match self {
            Principle::P1 => "P1",
            Principle::P2 => "P2",
            Principle::P3 => "P3",
            Principle::P4 => "P4",
            Principle::P5 => "P5",
            Principle::P6 => "P6",
            Principle::P7 => "P7",
            Principle::P8 => "P8",
            Principle::P9 => "P9",
            Principle::P10 => "P10",
            Principle::P11 => "P11",
            Principle::P12 => "P12",
            Principle::P13 => "P13",
        }
    }
}

/// 어긴 원칙 하나(E11). 어느 원칙인지는 [`Violation::principle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// P1: 한 주만 사도 1회 위험 한도를 넘는다(셈한 수량이 0).
    RiskTooLarge,
    /// P1: 장부 · 규칙 · 시세의 통화가 서로 다르다(통화는 섞지 않는다).
    CurrencyMismatch,
    /// P2: 손절값이 진입가보다 낮지 않다.
    StopNotBelowEntry,
    /// P2: 보유 중 손절을 넓히려 한다.
    StopWidened,
    /// P3: 그날 손실이 하루 손실 한도에 닿았다.
    DailyLossLimitReached,
    /// P4: 최고 평가액 대비 낙폭이 최대 낙폭 한도에 닿았다.
    DrawdownLimitReached,
    /// P5: 설정의 금지 종목이다.
    BannedSymbol,
    /// P5: 금지 상품 종류다(레버리지 · 인버스 · 파생 · 모름).
    BannedProduct(ProductKind),
    /// P5: 현금이 아닌 돈(미수 · 신용)이다.
    BannedFunding(Funding),
    /// P5: 가진 것보다 많이 팔려 한다(공매도). 가진 것이 없으면 막는다.
    ShortSale,
    /// P6: 손실 중인 종목을 더 사려 한다.
    AveragingDown,
    /// P8: 종목 비중이 상한에 닿아 더 살 수 없다.
    SymbolWeightFull,
    /// P8: 업종 비중이 상한에 닿아 더 살 수 없다.
    SectorWeightFull,
    /// P8: 업종표에 없는 종목이라 업종 비중을 셀 수 없다.
    UnknownSector,
    /// P10: 판단 시각보다 늦게 알게 된 자료를 썼다.
    DataFromFuture,
    /// P11 · N6: 시세가 늦었다.
    StaleQuote,
    /// P11: 계좌가 기록과 어긋난다.
    AccountMismatch,
    /// P11: 바깥 API 오류가 있다.
    ApiError,
    /// P11: 자료 이상이 있다.
    DataAnomaly,
    /// P11: 받은 시세의 종목이 주문 안의 종목과 다르다(사고팔기 모두 막는다).
    QuoteSymbolMismatch,
    /// P12 · E18: 장이 열려 있지 않다.
    MarketClosed,
    /// P12: 지금 세션에서 받지 않는 호가 유형이다.
    OrderTypeNotAllowed,
    /// P12: 값이 가격제한폭 밖이다.
    OutsidePriceLimit,
    /// P12: VI가 걸려 있다.
    ViActive,
    /// P12: 거래정지다.
    TradingHalted,
    /// P12: VI · 거래정지를 알 수 없다(결정 19번).
    StatusUnknown,
    /// P12: 같은 종목 · 같은 방향의 끝나지 않은 주문이 있다.
    DuplicateOrder,
    /// P12: 1회 최대 주문 금액을 넘는다.
    OverMaxOrderAmount,
    /// P12: 일일 최대 매매 횟수에 닿았다.
    TradeCountReached,
    /// P12: API 요청 한도에 닿았다.
    ApiLimitReached,
    /// P12 · 결정 7번: 시장가 사기가 자동 주문 금액 한도를 넘는다. 승인이 필요한 사는 주문은
    /// 지정가로만 낸다(승인 지문에 값이 들어가야 한다).
    MarketBuyOverAutoLimit,
    /// P13: 비용 · 세금 · 미끄러짐을 뺀 기대값이 양수가 아니다.
    NoPositiveEdge,
    /// P12 · E22: 답 없는 사기를 금액 한도 안으로 줄였더니 한 주도 살 수 없다(4차).
    NothingWithinLimit,
}

impl Violation {
    /// 이 위반이 어긴 원칙.
    pub fn principle(&self) -> Principle {
        match self {
            Violation::RiskTooLarge | Violation::CurrencyMismatch => Principle::P1,
            Violation::StopNotBelowEntry | Violation::StopWidened => Principle::P2,
            Violation::DailyLossLimitReached => Principle::P3,
            Violation::DrawdownLimitReached => Principle::P4,
            Violation::BannedSymbol
            | Violation::BannedProduct(_)
            | Violation::BannedFunding(_)
            | Violation::ShortSale => Principle::P5,
            Violation::AveragingDown => Principle::P6,
            Violation::SymbolWeightFull
            | Violation::SectorWeightFull
            | Violation::UnknownSector => Principle::P8,
            Violation::DataFromFuture => Principle::P10,
            Violation::StaleQuote
            | Violation::AccountMismatch
            | Violation::ApiError
            | Violation::DataAnomaly
            | Violation::QuoteSymbolMismatch => Principle::P11,
            Violation::MarketClosed
            | Violation::OrderTypeNotAllowed
            | Violation::OutsidePriceLimit
            | Violation::ViActive
            | Violation::TradingHalted
            | Violation::StatusUnknown
            | Violation::DuplicateOrder
            | Violation::OverMaxOrderAmount
            | Violation::TradeCountReached
            | Violation::ApiLimitReached
            | Violation::MarketBuyOverAutoLimit => Principle::P12,
            Violation::NoPositiveEdge => Principle::P13,
            Violation::NothingWithinLimit => todo!(),
        }
    }

    /// 사람이 읽는 한 줄 설명(원칙 번호는 붙이지 않는다).
    fn describe(&self) -> String {
        let text = match self {
            Violation::RiskTooLarge => "한 주도 1회 위험 한도 안에서 살 수 없다(셈한 수량이 0)",
            Violation::CurrencyMismatch => "장부 · 규칙 · 시세의 통화가 서로 다르다",
            Violation::StopNotBelowEntry => "손절값이 진입가보다 낮지 않다",
            Violation::StopWidened => "보유 중 손절을 넓히려 한다",
            Violation::DailyLossLimitReached => "그날 손실이 하루 손실 한도에 닿았다",
            Violation::DrawdownLimitReached => "최고 평가액 대비 낙폭이 최대 낙폭 한도에 닿았다",
            Violation::BannedSymbol => "설정의 금지 종목이다",
            Violation::BannedProduct(kind) => return format!("금지 상품 종류다({kind:?})"),
            Violation::BannedFunding(funding) => {
                return format!("현금이 아닌 돈으로 사려 한다({funding:?})");
            }
            Violation::ShortSale => "가진 것이 없이 팔려 한다(공매도)",
            Violation::AveragingDown => "손실 중인 종목을 더 사려 한다",
            Violation::SymbolWeightFull => "종목 비중이 상한에 닿았다",
            Violation::SectorWeightFull => "업종 비중이 상한에 닿았다",
            Violation::UnknownSector => "업종표에 없는 종목이라 업종 비중을 셀 수 없다",
            Violation::DataFromFuture => "판단 시각보다 늦게 알게 된 자료를 썼다",
            Violation::StaleQuote => "시세가 늦었다",
            Violation::AccountMismatch => "계좌가 기록과 어긋난다",
            Violation::ApiError => "바깥 API 오류가 있다",
            Violation::DataAnomaly => "자료 이상이 있다",
            Violation::QuoteSymbolMismatch => "받은 시세의 종목이 주문 안의 종목과 다르다",
            Violation::MarketClosed => "장이 열려 있지 않다",
            Violation::OrderTypeNotAllowed => "지금 세션에서 받지 않는 호가 유형이다",
            Violation::OutsidePriceLimit => "값이 가격제한폭 밖이다",
            Violation::ViActive => "변동성 완화장치(VI)가 걸려 있다",
            Violation::TradingHalted => "거래정지 종목이다",
            Violation::StatusUnknown => "VI · 거래정지 여부를 알 수 없다",
            Violation::DuplicateOrder => "같은 종목 · 같은 방향의 끝나지 않은 주문이 있다",
            Violation::OverMaxOrderAmount => "1회 최대 주문 금액을 넘는다",
            Violation::TradeCountReached => "일일 최대 매매 횟수에 닿았다",
            Violation::ApiLimitReached => "API 요청 한도에 닿았다",
            Violation::MarketBuyOverAutoLimit => {
                "시장가 사기가 자동 주문 금액 한도를 넘는다(승인이 필요한 사기는 지정가만)"
            }
            Violation::NoPositiveEdge => "비용 · 세금 · 미끄러짐을 뺀 기대값이 양수가 아니다",
            Violation::NothingWithinLimit => todo!(),
        };
        text.to_string()
    }
}

/// 막힌 이유의 종류. store의 `BlockKind`와 1:1이다(V26). 규칙 엔진이 내는 것은
/// 언제나 [`BlockKind::PrincipleOrRationale`]이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// 원칙 · 근거 부족.
    PrincipleOrRationale,
    /// 내가 승인을 거절했다.
    ApprovalRejected,
    /// 정한 시간 안에 하나도 체결되지 않아 주문을 취소했다.
    UnfilledCancelled,
}

impl BlockKind {
    /// 모든 종류. store의 `BlockKind::ALL`과 순서까지 같다.
    pub const ALL: [BlockKind; 3] = [
        BlockKind::PrincipleOrRationale,
        BlockKind::ApprovalRejected,
        BlockKind::UnfilledCancelled,
    ];
}

/// 막힌 까닭.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// E20: 판단됨이 아닌 매매의 주문 안이다.
    NotJudged(TradeStage),
    /// E31 · E24: 운용 중이 아닐 때의 사는 주문, 또는 전체 정지 중의 걸어 둔 손절 · 청산이
    /// 아닌 파는 주문(익절 · 팀장 판단 매도)이다(4차, 결정 2026-11-21 12번).
    TeamNotOperating(TeamStatus),
    /// E10 · P9: 빠진 근거 칸들([`RationaleField::ALL`] 순서). 손절값이 빠지면 P2도 어긴 것이다.
    MissingRationale(Vec<RationaleField>),
    /// E11: 어긴 원칙들(하나 이상).
    Violations(Vec<Violation>),
    /// 승인이 이 주문을 덮지 못한다: 승인 증표가 비었거나, 다시 점검한 주문 내용이
    /// 승인을 기다리던 내용과 다르다. 종류는 [`BlockKind::ApprovalRejected`]다.
    ApprovalMismatch,
}

impl Reason {
    /// 이 까닭이 어긴 원칙 번호들(작은 번호부터, 겹치지 않게).
    /// 근거가 빠지면 P9, 그 가운데 손절값이 있으면 P2도 들어간다. 판단 · 팀 상태 때문이면 비어 있다.
    pub fn principles(&self) -> Vec<Principle> {
        let mut found = match self {
            Reason::NotJudged(_) | Reason::TeamNotOperating(_) | Reason::ApprovalMismatch => {
                Vec::new()
            }
            Reason::MissingRationale(fields) => {
                let mut list = vec![Principle::P9];
                if fields.contains(&RationaleField::Stop) {
                    list.push(Principle::P2);
                }
                list
            }
            Reason::Violations(list) => list.iter().map(Violation::principle).collect(),
        };
        found.sort();
        found.dedup();
        found
    }
}

/// 막힘(V26: 막힌 주문마다 이유가 기록과 화면에 보인다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    /// store에 남길 종류.
    pub kind: BlockKind,
    /// 까닭.
    pub reason: Reason,
}

impl Blocked {
    /// 사람이 읽는 설명. 비어 있지 않고, 어긴 원칙이 있으면 그 번호 글자(`"P5"` 등)가 들어간다.
    pub fn detail(&self) -> String {
        match &self.reason {
            Reason::NotJudged(stage) => {
                format!("판단됨 상태가 아닌 매매의 주문 안은 받지 않는다(E20, 지금 {stage:?})")
            }
            Reason::TeamNotOperating(team) => {
                format!("투자 팀이 운용 중이 아니라 사는 주문을 받지 않는다(E31, 지금 {team:?})")
            }
            Reason::MissingRationale(fields) => {
                let codes: Vec<&str> = self
                    .reason
                    .principles()
                    .into_iter()
                    .map(Principle::code)
                    .collect();
                let names: Vec<&str> = fields.iter().map(|field| field_name(*field)).collect();
                format!("근거가 빠졌다({}): {}", codes.join(" · "), names.join(", "))
            }
            Reason::Violations(list) => {
                let lines: Vec<String> = list
                    .iter()
                    .map(|violation| {
                        format!("{} {}", violation.principle().code(), violation.describe())
                    })
                    .collect();
                format!("원칙을 어겼다: {}", lines.join("; "))
            }
            Reason::ApprovalMismatch => {
                "승인이 이 주문을 덮지 못한다(증표가 비었거나 다시 점검한 내용이 다르다)"
                    .to_string()
            }
        }
    }
}

/// 근거 칸의 이름.
fn field_name(field: RationaleField) -> &'static str {
    match field {
        RationaleField::Hypothesis => "가설",
        RationaleField::Trigger => "계기",
        RationaleField::Entry => "진입가",
        RationaleField::Stop => "손절값",
        RationaleField::Target => "목표값",
        RationaleField::ExpectedValue => "기대값",
    }
}

/// 점검을 다 지난 주문. 규칙 엔진만 만든다(칸이 비공개). `Clone`이 아니다: 한 번 지난
/// 점검으로 점검 통과 표시를 두 번 받을 수 없다.
#[derive(Debug, PartialEq, Eq)]
pub struct Passed {
    order: ApprovedOrder,
    amount: Decimal,
    approval: Option<ApprovalGrant>,
}

impl Passed {
    /// 이 크레이트 안에서만 만든다.
    pub(crate) fn new(order: ApprovedOrder, amount: Decimal) -> Self {
        Self {
            order,
            amount,
            approval: None,
        }
    }

    /// 승인 증표를 단다([`crate::approved`]).
    pub(crate) fn with_approval(mut self, grant: ApprovalGrant) -> Self {
        self.approval = Some(grant);
        self
    }

    /// 승인으로 지난 주문이면 그 승인 증표([`crate::approved`]). 한도 안이라 바로 지났으면 `None`.
    pub fn approval(&self) -> Option<&ApprovalGrant> {
        self.approval.as_ref()
    }

    /// 규칙 엔진이 셈한 주문 내용.
    pub fn order(&self) -> &ApprovedOrder {
        &self.order
    }

    /// 주문 내용을 꺼낸다(점검 통과 표시를 낼 때).
    pub(crate) fn into_order(self) -> ApprovedOrder {
        self.order
    }

    /// 주문 금액(수량 × 값). 시장가 사기는 시세 × (1 + 미끄러짐), 시장가 팔기는 시세로 센다.
    pub fn amount(&self) -> Decimal {
        self.amount
    }
}

/// 금액 한도를 넘어 승인을 기다려야 하는 주문(E21 · V25). 점검 통과 표시를 받을 수 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingApproval {
    order: ApprovedOrder,
    amount: Decimal,
    limit: Decimal,
}

impl PendingApproval {
    /// 이 크레이트 안에서만 만든다.
    pub(crate) fn new(order: ApprovedOrder, amount: Decimal, limit: Decimal) -> Self {
        Self {
            order,
            amount,
            limit,
        }
    }

    /// 규칙 엔진이 셈한 주문 내용.
    pub fn order(&self) -> &ApprovedOrder {
        &self.order
    }

    /// 주문 금액.
    pub fn amount(&self) -> Decimal {
        self.amount
    }

    /// 넘은 자동 주문 금액 한도(평가액 × 한도%).
    pub fn limit(&self) -> Decimal {
        self.limit
    }
}

/// 주문 전 점검의 결과(C9). [`Passed`]가 `Clone`이 아니라 이것도 `Clone`이 아니다.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// 다 지났다. [`crate::issue`]로 점검 통과 표시를 받는다.
    Pass(Passed),
    /// 사는 주문의 금액이 자동 주문 금액 한도를 넘는다(E21).
    NeedsApproval(PendingApproval),
    /// 막혔다.
    Blocked(Blocked),
}
