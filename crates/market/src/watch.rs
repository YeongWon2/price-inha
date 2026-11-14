//! 지켜보기 (V32).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

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
    todo!()
}
