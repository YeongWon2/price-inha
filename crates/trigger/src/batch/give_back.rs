//! (8차) 상한으로 버린 사건을 그 묶음이 나간 뒤 돌려준다([`crate::Trigger::take_returned_data`]).

use super::{DEDUPE_CAPACITY, DataState, Pending};
use crate::DataChange;

impl DataState {
    /// (8차) 상한으로 버린 사건을 돌려줄 때까지 둔다(그 묶음이 나가야 돌려준다 — 꽉 찬 묶음에 다시 넣어 곧바로 또
    /// 버리지 않게). 상한을 넘으면 가장 오래 둔 것부터 잊는다.
    pub(super) fn hold_back(&mut self, dropped: Vec<Pending>) {
        for pending in dropped {
            let key = (pending.change.market, pending.change.symbol.clone());
            self.waiting.push((key, pending.change));
        }
        let over = self.waiting.len().saturating_sub(DEDUPE_CAPACITY);
        self.waiting.drain(..over);
    }

    /// (8차) 묶음이 나간(묶음 · 장 밖 보관에 그 열쇠가 없는) 버린 사건을 돌려줄 것으로 옮기고 중복 표에서 뺀다.
    pub(super) fn release(&mut self) {
        let (ready, still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(key, _)| {
                !self.batches.iter().any(|batch| batch.key == *key)
                    && !self.held.iter().any(|(market, _)| *market == key.0)
            });
        self.waiting = still;
        for (_, change) in ready {
            let key = (
                change.source.clone(),
                change.id.clone(),
                change.revision.clone(),
            );
            self.seen.remove(&key);
            self.returned.push(change);
        }
        let over = self.returned.len().saturating_sub(DEDUPE_CAPACITY);
        self.returned.drain(..over);
    }

    /// (8차) 돌려줄 사건을 가져간다(한 번만 준다).
    pub(crate) fn take_returned(&mut self) -> Vec<DataChange> {
        std::mem::take(&mut self.returned)
    }
}
