//! (V32) 그날 누적 거래량: 지켜보는 종목마다 [`MarketAdapter::daily_volume`]을 시세보다 덜 자주 읽어 두고,
//! 시세 묶음(`quotes`)의 종목마다 `volume`(십진수 글자)으로 싣는다.
//!
//! - 증권사 현재가 응답에는 거래량 칸이 없다. 토스 어댑터는 일봉(`GET /api/v1/candles`, 종목 하나에 요청 하나,
//!   한도 그룹 `MARKET_DATA_CHART` 초당 20회)을 읽으므로 [`VOLUME_EVERY`]마다 한 번, 종목 사이에 [`VOLUME_GAP`]씩
//!   쉬며 읽는다. 읽는 일은 따로 돌아 시세 받기를 늦추지 않는다.
//! - 시연은 가짜 어댑터라 바깥 요청이 없다: 시세를 받을 때마다 곧바로 읽는다.
//! - 날(그 시장의 날짜)이 바뀌면 가진 값을 버리고 곧바로 다시 읽는다(어제 거래량을 싣지 않는다).
//! - 모르는 종목(`None` · 읽기 실패)은 칸을 두지 않는다(0으로 지어내지 않는다 — 기존 소비자 그대로).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use chrono::NaiveDate;
use pi_market::{MarketAdapter, MarketError, Symbol};
use rust_decimal::Decimal;
use serde_json::Value;
use tokio::task::JoinHandle;
use tokio::time::Instant;

/// 증권사 거래량을 다시 읽는 간격.
pub(crate) const VOLUME_EVERY: Duration = Duration::from_secs(60);
/// 증권사 거래량을 읽을 때 종목 사이에 쉬는 시간(초당 10회 아래 — 한도의 절반).
pub(crate) const VOLUME_GAP: Duration = Duration::from_millis(100);

#[derive(Default)]
struct State {
    /// 가진 값의 날짜.
    date: Option<NaiveDate>,
    by_symbol: HashMap<Symbol, Decimal>,
    /// 마지막으로 읽기 시작한 때.
    started: Option<Instant>,
    /// 읽는 중인가(겹쳐 읽지 않는다).
    running: bool,
}

/// 종목마다 그날 누적 거래량.
#[derive(Clone, Default)]
pub(crate) struct Volumes {
    state: Arc<Mutex<State>>,
}

impl Volumes {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 읽을 때가 됐으면 `symbols`의 `date` 거래량을 뒤에서 읽기 시작한다. 날이 바뀌었으면 가진 값을 버리고
    /// 곧바로 읽는다. 시작했으면 그 일을 돌려준다(시연은 기다렸다가 싣는다).
    pub(crate) fn refresh(
        &self,
        adapter: &Arc<dyn MarketAdapter>,
        symbols: Vec<Symbol>,
        date: NaiveDate,
        pace: (Duration, Duration),
    ) -> Option<JoinHandle<()>> {
        let (every, gap) = pace;
        {
            let mut state = self.state();
            if state.date != Some(date) {
                state.date = Some(date);
                state.by_symbol.clear();
                state.started = None;
            }
            let due = state.started.is_none_or(|at| at.elapsed() >= every);
            if state.running || !due || symbols.is_empty() {
                return None;
            }
            state.running = true;
            state.started = Some(Instant::now());
        }
        let volumes = self.clone();
        let adapter = Arc::clone(adapter);
        Some(tokio::spawn(async move {
            volumes
                .read_all(adapter.as_ref(), &symbols, date, gap)
                .await;
            volumes.state().running = false;
        }))
    }

    async fn read_all(
        &self,
        adapter: &dyn MarketAdapter,
        symbols: &[Symbol],
        date: NaiveDate,
        gap: Duration,
    ) {
        for (index, symbol) in symbols.iter().enumerate() {
            if index > 0 && !gap.is_zero() {
                tokio::time::sleep(gap).await;
            }
            let read = adapter.daily_volume(symbol, date).await;
            let mut state = self.state();
            if state.date != Some(date) {
                return; // 읽는 동안 날이 바뀌었다: 이 값들은 버린다.
            }
            match read {
                Ok(Some(volume)) => {
                    state.by_symbol.insert(symbol.clone(), volume);
                }
                Ok(None) => {
                    state.by_symbol.remove(symbol);
                }
                // 그 종목만 거절(종목 없음 등)이면 다음 종목을 읽는다. 그 밖(인증 · 한도 · 장애)은 이번에는 그만 읽는다.
                Err(MarketError::Rejected { .. }) => {
                    state.by_symbol.remove(symbol);
                }
                Err(error) => {
                    tracing::debug!(%error, "거래량을 읽지 못했습니다(다음 간격에 다시 읽습니다)");
                    return;
                }
            }
        }
    }

    /// 시세 묶음의 종목마다 `date`의 거래량을 `volume`으로 싣는다. 모르는 종목은 칸을 두지 않는다.
    pub(crate) fn decorate(&self, payload: &mut Value, date: NaiveDate) {
        let state = self.state();
        if state.date != Some(date) {
            return;
        }
        let Some(quotes) = payload.get_mut("quotes").and_then(Value::as_array_mut) else {
            return;
        };
        for quote in quotes {
            let Some(symbol) = quote.get("symbol").and_then(Value::as_str) else {
                continue;
            };
            if let Some(volume) = state.by_symbol.get(&Symbol::new(symbol))
                && let Some(fields) = quote.as_object_mut()
            {
                fields.insert("volume".to_string(), Value::String(volume.to_string()));
            }
        }
    }

    /// 시세 받기를 멈췄다: 가진 값을 버린다(다시 시작하면 곧바로 읽는다).
    pub(crate) fn forget(&self) {
        let mut state = self.state();
        state.date = None;
        state.by_symbol.clear();
        state.started = None;
    }
}

impl super::Feed {
    /// 시세를 받았으면 받은 종목들의 거래량을 읽을 때인지 본다. 시세를 하나도 못 받았으면(실패 중) 묻지 않는다.
    /// 시연은 바깥 요청이 없으므로 다 읽을 때까지 기다려 이번 묶음에 싣는다.
    pub(super) async fn read_volumes(
        &self,
        adapter: &Arc<dyn MarketAdapter>,
        fetched: &Result<super::Fetched, MarketError>,
        demo: bool,
    ) {
        let Ok(fetched) = fetched else {
            return;
        };
        let symbols: Vec<Symbol> = fetched.quotes.iter().map(|q| q.symbol.clone()).collect();
        let date = super::market_date(self.market, self.clock.now());
        let pace = if demo {
            (Duration::ZERO, Duration::ZERO)
        } else {
            (VOLUME_EVERY, VOLUME_GAP)
        };
        if let Some(reading) = self.volumes.refresh(adapter, symbols, date, pace)
            && demo
        {
            let _ = reading.await;
        }
    }
}
