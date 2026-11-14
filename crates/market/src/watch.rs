//! 지켜보기 (V32).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::{MarketAdapter, MarketError, Quote, Symbol};

/// 지켜보기 흐름으로 오는 것.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchEvent {
    /// 한 번 받은 시세들.
    Quotes(Vec<Quote>),
    /// 이번에는 받지 못했다. 흐름은 끊기지 않고 다음 간격에 다시 받는다.
    Failed(MarketError),
}

/// 간격마다 시세를 받아 흐름으로 보낸다.
///
/// - tokio 런타임 안에서 부른다. 작업 하나를 띄운다.
/// - 부르자마자 한 번 받고, 그 뒤로 `interval`마다 받는다. 간격은 tokio 시간
///   (`tokio::time`)으로 잰다.
/// - 받기에 실패하면 [`WatchEvent::Failed`]를 보내고 계속한다.
/// - 받는 쪽(`Receiver`)이 사라지면 멈추고 더는 어댑터를 부르지 않는다.
pub fn watch(
    adapter: Arc<dyn MarketAdapter>,
    symbols: Vec<Symbol>,
    interval: Duration,
) -> mpsc::Receiver<WatchEvent> {
    let (tx, rx) = mpsc::channel(WATCH_BUFFER);
    tokio::spawn(run(adapter, symbols, interval, tx));
    rx
}

/// 받는 쪽이 느릴 때 쌓아 두는 개수. 넘으면 받는 쪽이 가져갈 때까지 다음 받기를 미룬다.
const WATCH_BUFFER: usize = 8;

async fn run(
    adapter: Arc<dyn MarketAdapter>,
    symbols: Vec<Symbol>,
    interval: Duration,
    tx: mpsc::Sender<WatchEvent>,
) {
    // tokio의 interval은 0이면 panic한다. 0이 와도 죽지 않게 가장 짧은 간격으로 바꾼다.
    let mut ticker = tokio::time::interval(interval.max(Duration::from_millis(1)));
    // 받기가 간격보다 오래 걸려도 밀린 만큼 몰아서 받지 않는다.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        // 첫 tick은 바로 온다.
        tokio::select! {
            () = tx.closed() => return,
            _ = ticker.tick() => {}
        }
        let event = tokio::select! {
            () = tx.closed() => return,
            result = adapter.quotes(&symbols) => match result {
                Ok(quotes) => WatchEvent::Quotes(quotes),
                Err(error) => WatchEvent::Failed(error),
            },
        };
        if tx.send(event).await.is_err() {
            return;
        }
    }
}
