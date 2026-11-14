//! 한 번의 받기: 시세를 받고, 증권사가 거절한 종목을 가려내고, 필요하면 달력을 받는다.
//!
//! 여기의 함수는 시세 흐름의 상태를 쥐지 않는다(받는 동안에도 흐름이 늦은 시세를 볼 수 있게).

use std::sync::Arc;

use chrono::NaiveDate;
use pi_market::{MarketAdapter, MarketCalendar, MarketError, Quote, Symbol};

/// 한 번 받을 때 무엇을 물을 것인가.
pub(crate) struct Plan {
    /// 지켜보는 종목 전부.
    pub(crate) symbols: Vec<Symbol>,
    /// 증권사가 거절했다고 기억하는 종목. 빼고 묻는다.
    pub(crate) skip: Vec<Symbol>,
    /// 달력을 받을 날짜(받을 때가 아니면 없다). 시세를 하나라도 받았을 때만 받는다.
    pub(crate) calendar: Option<NaiveDate>,
}

impl Plan {
    /// 이번에 물을 종목(거절됐다고 기억하는 것은 뺀다).
    pub(crate) fn asking(&self) -> Vec<Symbol> {
        self.symbols
            .iter()
            .filter(|symbol| !self.skip.contains(symbol))
            .cloned()
            .collect()
    }
}

/// 한 번 받은 결과.
pub(crate) struct Fetched {
    /// 받은 시세.
    pub(crate) quotes: Vec<Quote>,
    /// 증권사가 거절한 종목(기억하던 것 포함).
    pub(crate) rejected: Vec<Symbol>,
    /// 달력을 받아 봤으면 그 날짜와 결과.
    pub(crate) calendar: Option<(NaiveDate, Result<MarketCalendar, MarketError>)>,
}

/// 가려낸 결과.
#[derive(Default)]
struct Sifted {
    quotes: Vec<Quote>,
    rejected: Vec<Symbol>,
}

/// 증권사가 요청을 거절한 것인가(4xx, 종목 없음 등). 인증 실패 · 한도 초과 · 장애는 아니다.
fn refused(error: &MarketError) -> bool {
    matches!(error, MarketError::Rejected { .. })
}

/// 거절된 종목을 가려내느라 한 번에 보내는 요청 수의 끝: 종목 수의 로그 정도.
/// 틀린 종목 하나를 찾는 데 드는 만큼이다. 더 있으면 다음 받기에서 이어서 가려낸다.
fn sift_budget(count: usize) -> u32 {
    let halvings = count.max(1).next_power_of_two().trailing_zeros();
    halvings.saturating_mul(2).saturating_add(2)
}

/// 통째로 거절된 묶음을 반으로 나눠 물어 틀린 종목을 가려낸다.
///
/// - 종목 하나짜리 묶음이 거절되면 그 종목이 거절된 것이다.
/// - 앞 절반이 받아졌으면 뒤 절반은 묻지 않아도 틀린 종목이 든 묶음이다.
/// - 요청 수가 [`sift_budget`]에 닿으면 멈춘다. 다 가려내지 못한 묶음은 이번에는 시세도 거절도
///   아니다(다음 받기가 가려낸 종목을 빼고 다시 물으면서 이어 간다).
/// - 거절이 아닌 실패(인증 · 한도 · 장애)를 만나면 그 실패로 끝낸다.
async fn sift(
    adapter: &dyn MarketAdapter,
    refused_group: Vec<Symbol>,
) -> Result<Sifted, MarketError> {
    let mut found = Sifted::default();
    let mut left = sift_budget(refused_group.len());
    // 통째로 거절된(틀린 종목이 든) 묶음들. 나중에 넣은 것부터 꺼내 한 갈래를 끝까지 좁힌다
    // (요청 수의 끝에 닿기 전에 틀린 종목 하나는 가려내게).
    let mut bad: Vec<Vec<Symbol>> = vec![refused_group];
    while let Some(mut group) = bad.pop() {
        if group.len() <= 1 {
            found.rejected.append(&mut group);
            continue;
        }
        if left == 0 {
            // 더 묻지 않는다. 이 묶음은 다음 받기로 넘긴다.
            continue;
        }
        let back = group.split_off(group.len() / 2);
        let front = group;
        left -= 1;
        match adapter.quotes(&front).await {
            Ok(quotes) => {
                found.quotes.extend(quotes);
                // 앞이 받아졌으니 틀린 종목은 뒤에 있다.
                bad.push(back);
            }
            Err(error) if refused(&error) => {
                if left > 0 {
                    left -= 1;
                    match adapter.quotes(&back).await {
                        Ok(quotes) => found.quotes.extend(quotes),
                        Err(error) if refused(&error) => bad.push(back),
                        Err(error) => return Err(error),
                    }
                }
                bad.push(front);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(found)
}

/// 시세를 받는다. 증권사가 묶음을 거절하면 틀린 종목을 가려내 나머지의 시세를 받는다.
async fn quotes(adapter: &dyn MarketAdapter, plan: &Plan) -> Result<Sifted, MarketError> {
    let asking = plan.asking();
    if asking.is_empty() {
        if plan.symbols.is_empty() {
            return Ok(Sifted::default());
        }
        // 모든 종목이 거절돼 있다. 가려내지 않고 통째로 한 번만 다시 물어본다.
        return match adapter.quotes(&plan.symbols).await {
            Ok(quotes) => Ok(Sifted {
                quotes,
                rejected: Vec::new(),
            }),
            Err(error) if refused(&error) => Ok(Sifted {
                quotes: Vec::new(),
                rejected: plan.skip.clone(),
            }),
            Err(error) => Err(error),
        };
    }
    let mut found = match adapter.quotes(&asking).await {
        Ok(quotes) => Sifted {
            quotes,
            rejected: Vec::new(),
        },
        Err(error) if refused(&error) => sift(adapter, asking).await?,
        Err(error) => return Err(error),
    };
    // 지켜보는 종목 순서로, 기억하던 것과 새로 가려낸 것을 합친다.
    found.rejected = plan
        .symbols
        .iter()
        .filter(|symbol| plan.skip.contains(symbol) || found.rejected.contains(symbol))
        .cloned()
        .collect();
    Ok(found)
}

/// 한 번 받는다: 시세, 그리고 시세를 하나라도 받았고 달력을 받을 때면 달력.
///
/// 달력은 증권사에 닿는 동안에만 받는다(실패 중에 요청을 더 보내지 않는다).
pub(crate) async fn fetch(
    adapter: Arc<dyn MarketAdapter>,
    plan: Plan,
) -> Result<Fetched, MarketError> {
    let found = quotes(adapter.as_ref(), &plan).await?;
    let calendar = match plan.calendar {
        Some(date) if !found.quotes.is_empty() => Some((date, adapter.calendar(date).await)),
        _ => None,
    };
    Ok(Fetched {
        quotes: found.quotes,
        rejected: found.rejected,
        calendar,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::Utc;
    use pi_market::{Currency, Market};
    use rust_decimal::Decimal;

    use super::*;

    /// 틀린 종목이 든 요청을 통째로 거절하는 가짜 증권사. 받은 요청을 적어 둔다.
    struct Picky {
        unknown: Vec<Symbol>,
        asked: Mutex<Vec<Vec<Symbol>>>,
    }

    impl Picky {
        fn new(unknown: &[&str]) -> Self {
            Self {
                unknown: unknown.iter().map(|code| Symbol::new(*code)).collect(),
                asked: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> usize {
            self.asked.lock().map_or(0, |asked| asked.len())
        }
    }

    #[async_trait]
    impl MarketAdapter for Picky {
        fn market(&self) -> Market {
            Market::Kr
        }

        async fn quotes(&self, symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
            if let Ok(mut asked) = self.asked.lock() {
                asked.push(symbols.to_vec());
            }
            if symbols.iter().any(|symbol| self.unknown.contains(symbol)) {
                return Err(MarketError::Rejected {
                    status: 404,
                    code: "stock-not-found".to_owned(),
                });
            }
            Ok(symbols
                .iter()
                .map(|symbol| Quote {
                    symbol: symbol.clone(),
                    price: Decimal::ONE,
                    currency: Currency::Krw,
                    exchange_time: None,
                    received_at: Utc::now(),
                })
                .collect())
        }

        async fn calendar(&self, _date: NaiveDate) -> Result<MarketCalendar, MarketError> {
            Err(MarketError::Unavailable {
                status: 503,
                code: None,
            })
        }
    }

    fn many(count: usize) -> Vec<Symbol> {
        (0..count)
            .map(|n| Symbol::new(format!("S{n:03}")))
            .collect()
    }

    fn plan(symbols: &[Symbol], skip: &[Symbol]) -> Plan {
        Plan {
            symbols: symbols.to_vec(),
            skip: skip.to_vec(),
            calendar: None,
        }
    }

    fn codes(symbols: &[Symbol]) -> Vec<&str> {
        symbols.iter().map(Symbol::as_str).collect()
    }

    #[tokio::test]
    async fn 틀린_종목_하나를_로그만큼의_요청으로_가려내고_나머지는_받는다() {
        let symbols = many(200);
        let picky = Picky::new(&["S137"]);
        let found = quotes(&picky, &plan(&symbols, &[])).await;
        let Ok(found) = found else {
            panic!("거절은 가려내고 끝나야 한다");
        };
        assert_eq!(codes(&found.rejected), ["S137"]);
        assert_eq!(found.quotes.len(), 199, "나머지 종목의 시세는 모두 받는다");
        // 통째로 한 번 + 가려내기(2·log2(200) + 2 = 18번 이하).
        assert!(picky.requests() <= 19, "{}번", picky.requests());
    }

    #[tokio::test]
    async fn 틀린_종목이_많아도_한_번에_보내는_요청에는_끝이_있고_다음에_이어_간다() {
        let symbols = many(64);
        let unknown = ["S003", "S017", "S029", "S040", "S055", "S063"];
        let picky = Picky::new(&unknown);
        let mut skip: Vec<Symbol> = Vec::new();
        for round in 1..=unknown.len() {
            let before = picky.requests();
            let Ok(found) = quotes(&picky, &plan(&symbols, &skip)).await else {
                panic!("거절은 가려내고 끝나야 한다");
            };
            let spent = picky.requests() - before;
            assert!(spent <= 15, "{round}번째 받기에 {spent}번을 보냈다");
            assert!(
                found.rejected.len() > skip.len() || found.rejected.len() == unknown.len(),
                "받을 때마다 틀린 종목을 하나는 더 가려낸다"
            );
            skip = found.rejected;
        }
        assert_eq!(codes(&skip), unknown);
        // 다 가려낸 뒤에는 한 번에 받는다.
        let before = picky.requests();
        let Ok(found) = quotes(&picky, &plan(&symbols, &skip)).await else {
            panic!("가려낸 뒤에는 받아야 한다");
        };
        assert_eq!(picky.requests() - before, 1);
        assert_eq!(found.quotes.len(), symbols.len() - unknown.len());
    }

    #[tokio::test]
    async fn 모든_종목이_거절돼_있으면_통째로_한_번만_다시_물어본다() {
        let symbols = many(8);
        let picky = Picky::new(&[
            "S000", "S001", "S002", "S003", "S004", "S005", "S006", "S007",
        ]);
        let Ok(found) = quotes(&picky, &plan(&symbols, &symbols)).await else {
            panic!("거절은 실패가 아니다");
        };
        assert_eq!(picky.requests(), 1, "가려내느라 두드리지 않는다");
        assert_eq!(found.rejected, symbols);
        assert!(found.quotes.is_empty());
    }

    #[tokio::test]
    async fn 거절이_아닌_실패는_그대로_돌려준다() {
        struct Down;
        #[async_trait]
        impl MarketAdapter for Down {
            fn market(&self) -> Market {
                Market::Kr
            }
            async fn quotes(&self, _symbols: &[Symbol]) -> Result<Vec<Quote>, MarketError> {
                Err(MarketError::IpNotAllowed)
            }
            async fn calendar(&self, _date: NaiveDate) -> Result<MarketCalendar, MarketError> {
                Err(MarketError::IpNotAllowed)
            }
        }
        let found = fetch(Arc::new(Down), plan(&many(3), &[])).await;
        assert!(matches!(found, Err(MarketError::IpNotAllowed)));
    }
}
