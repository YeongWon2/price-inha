//! 설정. 시장마다 숫자 규칙(수수료 · 세금 · 호가 단위 · 상하한 · 결제일 · 미끄러짐 ·
//! 세션별 허용)을 [`MarketRules`] 하나에 모은다. 기본값마다 출처를 적는다.

use pi_market::{Currency, Market, SessionKind, StalePolicy};
use rust_decimal::Decimal;

use crate::{BrokerError, OrderType};

/// 반올림 방향.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoundingMode {
    /// 버림(0 쪽으로).
    Down,
    /// 반올림(딱 절반이면 0에서 먼 쪽).
    HalfUp,
    /// 올림(0에서 먼 쪽).
    Up,
}

/// 돈의 반올림 규칙. 이 모듈의 반올림은 모두 여기 한 곳을 거친다.
///
/// - 체결마다 떼는 수수료와 세금: `금액 × 비율`을 `scale` 자리에서 `mode`로 맞춘다.
/// - 사는 주문에 묶는 돈의 수수료 몫: 같은 `scale`에서 **항상 올린다**(모자라게 묶지 않는다).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoneyRounding {
    /// 소수 자릿수. 원화는 0, 달러는 2.
    pub scale: u32,
    /// 수수료 · 세금을 맞추는 방향.
    pub mode: RoundingMode,
}

/// 호가 단위 표의 한 줄.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickBand {
    /// 값이 이 값 **미만**이면 이 줄을 쓴다. `None`이면 나머지 전부(표의 마지막 줄).
    pub below: Option<Decimal>,
    /// 그 구간의 호가 단위.
    pub tick: Decimal,
}

/// 지정가 주문이 체결될 때의 값.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitFillPrice {
    /// 지정가로 체결한다. 시세가 더 유리해도 그 이득을 주지 않는다(보수적). 기본값.
    Limit,
    /// 그때의 시세로 체결한다(지정가보다 유리할 수 있다).
    Quote,
}

/// 시세 한 번에 주문 하나가 체결되는 최대 수량.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaxFill {
    /// 남은 수량 전부.
    Unlimited,
    /// 주문마다 시세 한 번에 이 수량까지. 남으면 일부 체결이 된다.
    PerQuote(Decimal),
}

/// 판 돈을 언제 다시 쓸 수 있는가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProceedsUsable {
    /// 체결되자마자 쓸 수 있다.
    Immediately,
    /// 결제일이 돼야 쓸 수 있다. 그때까지는 [`crate::CashBalance::unsettled`]에 있다.
    AfterSettlement,
}

/// 결제 규칙.
///
/// 결제 예정일 = 체결한 영업일의 날짜 + `business_days` 영업일. 체결한 영업일은 달력에서
/// 체결 시각이 든 세션의 날([`pi_market::TradingDay::date`])이다. 영업일은 **평일(월~금)로만**
/// 센다. `pi-market`의 달력은 앞뒤 하루씩만 알려 줘서 그 사이의 휴장일은 반영하지 못한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settlement {
    /// 체결일 뒤 몇 영업일째에 결제되는가. 국내 2(T+2), 미국 1(T+1).
    pub business_days: u32,
    /// 판 돈을 언제 다시 쓸 수 있는가.
    pub proceeds_usable: ProceedsUsable,
}

/// 세션별로 받는 호가 유형. 여기 없는 세션에서는 그 유형을 받지도 체결하지도 않는다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRules {
    /// 지정가를 받는 세션들.
    pub limit: Vec<SessionKind>,
    /// 시장가를 받는 세션들.
    pub market: Vec<SessionKind>,
}

impl SessionRules {
    /// 정규장에서만 지정가 · 시장가를 받는다(기본).
    fn regular_only() -> Self {
        Self {
            limit: vec![SessionKind::Regular],
            market: vec![SessionKind::Regular],
        }
    }

    /// 그 세션에서 그 호가 유형을 받는가.
    pub fn allows(&self, session: SessionKind, order_type: OrderType) -> bool {
        match order_type {
            OrderType::Limit => self.limit.contains(&session),
            OrderType::Market => self.market.contains(&session),
        }
    }
}

/// 시장 하나의 규칙.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketRules {
    /// 이 시장의 통화. 이 통화의 현금으로만 사고판다(환전 없음).
    pub currency: Currency,
    /// 수수료율(사고팔 때 모두, 소수 비율).
    pub commission_rate: Decimal,
    /// 팔 때 떼는 세율(국내 증권거래세 등, 소수 비율). 살 때는 떼지 않는다.
    pub sell_tax_rate: Decimal,
    /// 돈의 반올림.
    pub rounding: MoneyRounding,
    /// 호가 단위 표. 위에서부터 보고 처음 맞는 줄을 쓴다.
    pub tick_table: Vec<TickBand>,
    /// 가격제한폭(소수 비율). `Some(r)`이면 기준가 × (1 ± r) 밖의 값은 받지 않는다.
    /// 상한가는 호가 단위 아래로, 하한가는 위로 맞춘다(범위 안쪽). `None`이면 제한이 없다.
    pub price_limit_rate: Option<Decimal>,
    /// 결제 규칙.
    pub settlement: Settlement,
    /// 시장가의 미끄러짐(소수 비율). 사기는 시세 × (1 + 비율), 팔기는 시세 × (1 − 비율)이고,
    /// 그 값을 불리한 쪽 호가 단위로 맞춘다(사기는 올림, 팔기는 내림).
    pub market_slippage_rate: Decimal,
    /// 지정가가 체결될 때의 값.
    pub limit_fill_price: LimitFillPrice,
    /// 시세 한 번에 체결되는 최대 수량.
    pub max_fill: MaxFill,
    /// 세션별로 받는 호가 유형.
    pub sessions: SessionRules,
}

impl MarketRules {
    /// 국내 주식의 기본 규칙.
    ///
    /// | 값 | 기본 | 출처 |
    /// |---|---|---|
    /// | 통화 | KRW, 소수 0자리 | 원화에는 원 아래 단위가 없다 |
    /// | 호가 단위 | 2,000 미만 1 · 5,000 미만 5 · 20,000 미만 10 · 50,000 미만 50 · 200,000 미만 100 · 500,000 미만 500 · 그 이상 1,000 | KRX 공식 안내(global.krx.co.kr "Tick Size", 2026-11-14 확인). 토스 명세의 예(50,000~200,000원은 100원)와 맞는다. ETF · ELW · 넥스트레이드의 다른 표는 넣지 않았다 |
    /// | 가격제한폭 | 0.30 | `docs/research/stock-investing-principles.md` 5.1 |
    /// | 증권거래세 | 0.0020 (코스피 · 코스닥 같음, 2026년) | 같은 문서 5.1. 코넥스(0.10%)는 나누지 않았다 |
    /// | 결제일 | 2영업일 (T+2) | 같은 문서 5.1 |
    /// | 판 돈 | 결제일 뒤에 쓴다(보수적) | **미확인**. 실제 증권사의 재매수 가능 여부는 확인하지 못했다 |
    /// | 수수료율 | 0 | **확인되지 않은 값이며 사용자가 설정해야 한다.** 토스증권 수수료 안내를 확인하지 못해 지어내지 않고 0으로 뒀다 |
    /// | 반올림 방향 | [`RoundingMode::Up`] | **확인되지 않은 값이며 사용자가 설정해야 한다.** 가장 보수적인 쪽(수수료 · 세금을 더 뗀다)으로 뒀다 |
    /// | 미끄러짐 | 0 | **확인되지 않은 값이며 사용자가 설정해야 한다.** 근거 있는 값이 없어 0으로 뒀다 |
    /// | 지정가 체결 값 | [`LimitFillPrice::Limit`] | 지시서(보수적인 쪽) |
    /// | 최대 체결 수량 | [`MaxFill::Unlimited`] | 지시서(설정) |
    /// | 세션 | 정규장에서만 지정가 · 시장가 | 보수적으로 잡았다. 프리 · 애프터마켓의 허용 유형은 **미확인** |
    pub fn kr() -> Self {
        let band = |below: Option<i64>, tick: i64| TickBand {
            below: below.map(Decimal::from),
            tick: Decimal::from(tick),
        };
        Self {
            currency: Currency::Krw,
            commission_rate: Decimal::ZERO,
            sell_tax_rate: Decimal::new(20, 4),
            rounding: MoneyRounding {
                scale: 0,
                mode: RoundingMode::Up,
            },
            tick_table: vec![
                band(Some(2_000), 1),
                band(Some(5_000), 5),
                band(Some(20_000), 10),
                band(Some(50_000), 50),
                band(Some(200_000), 100),
                band(Some(500_000), 500),
                band(None, 1_000),
            ],
            price_limit_rate: Some(Decimal::new(30, 2)),
            settlement: Settlement {
                business_days: 2,
                proceeds_usable: ProceedsUsable::AfterSettlement,
            },
            market_slippage_rate: Decimal::ZERO,
            limit_fill_price: LimitFillPrice::Limit,
            max_fill: MaxFill::Unlimited,
            sessions: SessionRules::regular_only(),
        }
    }

    /// 미국 주식의 기본 규칙.
    ///
    /// | 값 | 기본 | 출처 |
    /// |---|---|---|
    /// | 통화 | USD, 소수 2자리 | 센트 |
    /// | 호가 단위 | 1달러 미만 0.0001 · 그 이상 0.01 | 토스 명세의 주문 생성 요청 `price` 설명 |
    /// | 가격제한폭 | 없음 | 토스 명세의 상/하한가 응답(미국은 null), 조사 문서 5.2 |
    /// | 결제일 | 1영업일 (T+1) | 조사 문서 5.2 |
    /// | 팔 때 세금(SEC Fee 등) | 0 | **확인되지 않은 값이며 사용자가 설정해야 한다** |
    /// | 수수료율 | 0 | **확인되지 않은 값이며 사용자가 설정해야 한다** |
    /// | 반올림 방향 | [`RoundingMode::Up`] | **확인되지 않은 값이며 사용자가 설정해야 한다.** 가장 보수적인 쪽으로 뒀다 |
    /// | 미끄러짐 | 0 | **확인되지 않은 값이며 사용자가 설정해야 한다** |
    /// | 그 밖 | 국내와 같다 | |
    pub fn us() -> Self {
        Self {
            currency: Currency::Usd,
            commission_rate: Decimal::ZERO,
            sell_tax_rate: Decimal::ZERO,
            rounding: MoneyRounding {
                scale: 2,
                mode: RoundingMode::Up,
            },
            tick_table: vec![
                TickBand {
                    below: Some(Decimal::ONE),
                    tick: Decimal::new(1, 4),
                },
                TickBand {
                    below: None,
                    tick: Decimal::new(1, 2),
                },
            ],
            price_limit_rate: None,
            settlement: Settlement {
                business_days: 1,
                proceeds_usable: ProceedsUsable::AfterSettlement,
            },
            market_slippage_rate: Decimal::ZERO,
            limit_fill_price: LimitFillPrice::Limit,
            max_fill: MaxFill::Unlimited,
            sessions: SessionRules::regular_only(),
        }
    }

    /// 그 값 구간의 호가 단위. 맞는 줄이 없으면 표의 마지막 줄을 쓰고, 표가 비어 있으면 0이다
    /// (0은 "호가 단위 없음"으로 다룬다. 계좌를 열 때 빈 표는 받지 않는다).
    pub fn tick_for(&self, price: Decimal) -> Decimal {
        self.tick_table
            .iter()
            .find(|band| band.below.is_none_or(|below| price < below))
            .or(self.tick_table.last())
            .map_or(Decimal::ZERO, |band| band.tick)
    }
}

/// 처음 돈. 통화별로 따로 둔다(환전 없음).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitialCash {
    /// 원화.
    pub krw: Decimal,
    /// 달러.
    pub usd: Decimal,
}

/// 모의계좌 설정.
#[derive(Debug, Clone, PartialEq)]
pub struct PaperConfig {
    /// 처음 돈. 되살릴 때([`crate::PaperBroker::restore`])는 쓰지 않는다.
    pub initial_cash: InitialCash,
    /// 국내 주식 규칙.
    pub kr: MarketRules,
    /// 미국 주식 규칙.
    pub us: MarketRules,
    /// 늦은 시세 기준 (N6). 이 기준으로 늦은 시세로는 체결하지 않는다.
    pub stale: StalePolicy,
}

impl PaperConfig {
    /// 기본 규칙([`MarketRules::kr`] · [`MarketRules::us`] · [`StalePolicy::default`])에 처음 돈만 정한 설정.
    pub fn new(initial_cash: InitialCash) -> Self {
        Self {
            initial_cash,
            kr: MarketRules::kr(),
            us: MarketRules::us(),
            stale: StalePolicy::default(),
        }
    }

    /// 그 시장의 규칙. 다루지 않는 시장이면 `None`.
    pub fn rules(&self, market: Market) -> Option<&MarketRules> {
        match market {
            Market::Kr => Some(&self.kr),
            Market::Us => Some(&self.us),
            _ => None,
        }
    }
}

impl MarketRules {
    /// 규칙 값이 쓸 수 있는 모양인가. 틀린 곳이 있으면 무엇이 틀렸는지 돌려준다.
    fn validate(&self, name: &str) -> Result<(), BrokerError> {
        let bad = |what: &str| Err(BrokerError::InvalidConfig(format!("{name}: {what}")));
        if !matches!(self.currency, Currency::Krw | Currency::Usd) {
            return bad("통화는 KRW나 USD여야 합니다");
        }
        let rates = [
            self.commission_rate,
            self.sell_tax_rate,
            self.market_slippage_rate,
            self.price_limit_rate.unwrap_or(Decimal::ZERO),
        ];
        if rates.iter().any(Decimal::is_sign_negative) {
            return bad("비율은 음수일 수 없습니다");
        }
        if self.rounding.scale > MAX_MONEY_SCALE {
            return bad("돈의 소수 자릿수가 너무 큽니다");
        }
        if self.tick_table.is_empty()
            || self
                .tick_table
                .iter()
                .any(|band| band.tick <= Decimal::ZERO)
        {
            return bad("호가 단위 표가 비었거나 0 이하의 단위가 있습니다");
        }
        if let MaxFill::PerQuote(quantity) = &self.max_fill
            && (*quantity <= Decimal::ZERO || !quantity.fract().is_zero())
        {
            return bad("한 번에 체결되는 최대 수량은 양의 정수여야 합니다");
        }
        Ok(())
    }
}

/// 돈의 소수 자릿수 한도. 이보다 크면 십진수 계산이 넘칠 수 있다.
const MAX_MONEY_SCALE: u32 = 8;

impl PaperConfig {
    /// 설정 값이 쓸 수 있는 모양인가(비율 · 호가 단위 표 · 통화). 처음 돈은 여기서 보지 않는다.
    pub(crate) fn validate_rules(&self) -> Result<(), BrokerError> {
        self.kr.validate("국내 규칙")?;
        self.us.validate("미국 규칙")
    }

    /// 처음 돈이 음수가 아닌가.
    pub(crate) fn validate_initial_cash(&self) -> Result<(), BrokerError> {
        if self.initial_cash.krw.is_sign_negative() || self.initial_cash.usd.is_sign_negative() {
            return Err(BrokerError::InvalidConfig(
                "처음 돈은 음수일 수 없습니다".to_owned(),
            ));
        }
        Ok(())
    }
}
