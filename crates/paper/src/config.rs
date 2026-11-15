//! 설정. 시장마다 숫자 규칙(수수료 · 세금 · 호가 단위 · 상하한 · 결제일 · 미끄러짐 ·
//! 세션별 허용 · 큰 주문)을 [`MarketRules`] 하나에 모은다. 기본값마다 출처를 적는다.
//! 공식 출처로 확인하지 못한 숫자에는 기본값이 없다([`UnverifiedRules`]).

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
/// - 떼는 수수료와 세금: 주문의 `누적 체결 금액 × 비율`을 `scale` 자리에서 `mode`로 맞춘 값이
///   그 주문에서 뗀 합이 된다(체결마다 그 값에서 이미 뗀 것을 뺀 만큼 뗀다).
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

/// 공식 출처로 확인하지 못해 **기본값이 없는** 숫자들. 계좌를 열 때 분명히 줘야 한다.
///
/// [`MarketRules::kr`] · [`MarketRules::us`]가 이 값을 인자로 받는다. 주지 않고는 규칙을 만들 수
/// 없으니 계좌도 열 수 없다(조용히 0으로 돌지 않는다). 준 값은 그대로 규칙에 들어간다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedRules {
    /// 수수료율(소수 비율). → [`MarketRules::commission_rate`].
    pub commission_rate: Decimal,
    /// 시장가의 미끄러짐(소수 비율). → [`MarketRules::market_slippage_rate`].
    pub market_slippage_rate: Decimal,
    /// 수수료 · 세금의 반올림 방향. → [`MoneyRounding::mode`].
    pub rounding_mode: RoundingMode,
    /// 시세 한 번에 체결되는 최대 수량. → [`MarketRules::max_fill`].
    pub max_fill: MaxFill,
    /// 판 돈을 다시 쓸 수 있는 때. → [`Settlement::proceeds_usable`].
    pub proceeds_usable: ProceedsUsable,
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
    /// 큰 주문의 금액 확인. `Some(a)`이면 주문 금액(수량 × 값)이 `a` **이상**인 주문은
    /// 확인 표시([`crate::OrderRequest::confirm_high_value`])가 있어야 받는다. 없으면
    /// [`BrokerError::ConfirmHighValueRequired`]. `None`이면 보지 않는다.
    ///
    /// 주문 금액: 지정가는 `수량 × 지정가`, 시장가 사기는 `수량 × 묶는 돈을 셈하는 값`.
    /// 시장가 팔기는 값이 없어 보지 않는다(명세에 셈법이 없다. **미확인**).
    pub high_value_confirm_from: Option<Decimal>,
    /// 최대 주문 금액. `Some(a)`이면 주문 금액이 `a`를 **넘는** 주문은 확인 표시와 상관없이
    /// 받지 않는다([`BrokerError::MaxOrderAmountExceeded`]). `None`이면 보지 않는다.
    pub max_order_amount: Option<Decimal>,
}

impl MarketRules {
    /// 국내 주식의 규칙. 공식 출처로 확인된 값만 기본값이고, 확인되지 않은 값은 `unverified`로 받는다.
    ///
    /// | 값 | 기본 | 출처 |
    /// |---|---|---|
    /// | 통화 | KRW, 소수 0자리 | 원화에는 원 아래 단위가 없다 |
    /// | 호가 단위 | 2,000 미만 1 · 5,000 미만 5 · 20,000 미만 10 · 50,000 미만 50 · 200,000 미만 100 · 500,000 미만 500 · 그 이상 1,000 | KRX 공식 안내(global.krx.co.kr "Tick Size", 2026-11-14 확인). 토스 명세의 예(50,000~200,000원은 100원)와 맞는다. ETF · ELW · 넥스트레이드의 다른 표는 넣지 않았다 |
    /// | 가격제한폭 | 0.30 | `docs/research/stock-investing-principles.md` 5.1 |
    /// | 증권거래세 | 0.0020 (코스피 · 코스닥 같음, 2026년) | 같은 문서 5.1. 코넥스(0.10%)는 나누지 않았다 |
    /// | 결제일 | 2영업일 (T+2) | 같은 문서 5.1 |
    /// | 큰 주문 확인 | 100,000,000원 이상 | 토스 명세의 주문 생성 · 정정 요청 `confirmHighValueOrder` 설명과 `confirm-high-value-required` 오류 예(`threshold` 100000000, KRW) |
    /// | 최대 주문 금액 | 3,000,000,000원 초과는 거부 | 토스 명세의 `max-order-amount-exceeded` 오류 예("30억원 초과 주문 (KR)", `maxAmount` 3000000000). 명세의 정정 요청 설명 한 곳은 "30억원 이상"이라 적어 딱 30억 원이 어느 쪽인지는 **미확인** |
    /// | 지정가 체결 값 | [`LimitFillPrice::Limit`] | 지시서(보수적인 쪽) |
    /// | 세션 | 정규장에서만 지정가 · 시장가 | 보수적으로 잡았다. 프리 · 애프터마켓의 허용 유형은 **미확인** |
    /// | 수수료율 · 미끄러짐 · 반올림 방향 · 최대 체결 수량 · 판 돈을 쓰는 때 | **기본값 없음** | `unverified`에 준 값이 그대로 들어간다 |
    pub fn kr(unverified: UnverifiedRules) -> Self {
        let band = |below: Option<i64>, tick: i64| TickBand {
            below: below.map(Decimal::from),
            tick: Decimal::from(tick),
        };
        Self {
            currency: Currency::Krw,
            commission_rate: unverified.commission_rate,
            sell_tax_rate: Decimal::new(20, 4),
            rounding: MoneyRounding {
                scale: 0,
                mode: unverified.rounding_mode,
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
                proceeds_usable: unverified.proceeds_usable,
            },
            market_slippage_rate: unverified.market_slippage_rate,
            limit_fill_price: LimitFillPrice::Limit,
            max_fill: unverified.max_fill,
            sessions: SessionRules::regular_only(),
            high_value_confirm_from: Some(Decimal::from(100_000_000_i64)),
            max_order_amount: Some(Decimal::from(3_000_000_000_i64)),
        }
    }

    /// 미국 주식의 규칙. 확인되지 않은 값은 `unverified`와 `sell_cost_rate`로 받는다.
    ///
    /// | 값 | 기본 | 출처 |
    /// |---|---|---|
    /// | 통화 | USD, 소수 2자리 | 센트 |
    /// | 호가 단위 | 1달러 미만 0.0001 · 그 이상 0.01 | 토스 명세의 주문 생성 요청 `price` 설명 |
    /// | 가격제한폭 | 없음 | 토스 명세의 상/하한가 응답(미국은 null), 조사 문서 5.2 |
    /// | 결제일 | 1영업일 (T+1) | 조사 문서 5.2 |
    /// | 큰 주문 확인 · 최대 주문 금액 | 없음(`None`) | 명세의 기준은 원화 금액이다. 환전을 하지 않아 달러 주문의 원화 금액을 셈할 수 없다. 미국 주문에 어떻게 적용되는지는 **미확인** |
    /// | 팔 때 비용(SEC Fee 등) | **기본값 없음** | `sell_cost_rate`에 준 값이 그대로 [`MarketRules::sell_tax_rate`]에 들어간다 |
    /// | 수수료율 · 미끄러짐 · 반올림 방향 · 최대 체결 수량 · 판 돈을 쓰는 때 | **기본값 없음** | `unverified`에 준 값이 그대로 들어간다 |
    /// | 그 밖 | 국내와 같다 | |
    pub fn us(unverified: UnverifiedRules, sell_cost_rate: Decimal) -> Self {
        Self {
            currency: Currency::Usd,
            commission_rate: unverified.commission_rate,
            sell_tax_rate: sell_cost_rate,
            rounding: MoneyRounding {
                scale: 2,
                mode: unverified.rounding_mode,
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
                proceeds_usable: unverified.proceeds_usable,
            },
            market_slippage_rate: unverified.market_slippage_rate,
            limit_fill_price: LimitFillPrice::Limit,
            max_fill: unverified.max_fill,
            sessions: SessionRules::regular_only(),
            high_value_confirm_from: None,
            max_order_amount: None,
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
    /// 늦은 시세 기준 (N6). 이 기준으로 늦은 시세로는 체결하지 않고, 기억하지도 않는다.
    pub stale: StalePolicy,
    /// 체결 알림 구독 하나가 쥐는 이벤트의 최대 개수 (N1). 받는 쪽이 읽지 않아 이 개수가 차면
    /// 그 구독만 끊는다. 1 이상이어야 한다(0이면 [`BrokerError::InvalidConfig`]).
    pub event_buffer: usize,
}

/// [`PaperConfig::event_buffer`]의 기본값.
pub const DEFAULT_EVENT_BUFFER: usize = 1024;

impl PaperConfig {
    /// 처음 돈과 두 시장의 규칙으로 설정을 만든다. 규칙은 [`MarketRules::kr`] · [`MarketRules::us`]로
    /// 만들어 준다(확인되지 않은 숫자는 거기서 분명히 받는다). 늦은 시세 기준은
    /// [`StalePolicy::default`](30초, N6), 구독이 쥐는 개수는 [`DEFAULT_EVENT_BUFFER`]다.
    pub fn new(initial_cash: InitialCash, kr: MarketRules, us: MarketRules) -> Self {
        Self {
            initial_cash,
            kr,
            us,
            stale: StalePolicy::default(),
            event_buffer: DEFAULT_EVENT_BUFFER,
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
        // 3차 규칙 6: 수수료율 · 세율 · 미끄러짐이 1(100%) 이상이면 말이 안 된다.
        let charges = [
            self.commission_rate,
            self.sell_tax_rate,
            self.market_slippage_rate,
        ];
        if charges.iter().any(|rate| *rate >= Decimal::ONE) {
            return bad("수수료율 · 세율 · 미끄러짐은 1(100%)보다 작아야 합니다");
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
        let amounts = [self.high_value_confirm_from, self.max_order_amount];
        if amounts.iter().flatten().any(Decimal::is_sign_negative) {
            return bad("큰 주문의 기준 금액은 음수일 수 없습니다");
        }
        Ok(())
    }
}

/// 돈의 소수 자릿수 한도. 이보다 크면 십진수 계산이 넘칠 수 있다.
const MAX_MONEY_SCALE: u32 = 8;

impl PaperConfig {
    /// 설정 값이 쓸 수 있는 모양인가(비율 · 호가 단위 표 · 통화 · 구독이 쥐는 개수).
    /// 처음 돈은 여기서 보지 않는다.
    pub(crate) fn validate_rules(&self) -> Result<(), BrokerError> {
        if self.event_buffer == 0 {
            return Err(BrokerError::InvalidConfig(
                "구독이 쥐는 이벤트 개수는 1 이상이어야 합니다".to_owned(),
            ));
        }
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
