//! (68차, 백로그 U 3번 "본 것 기억 기록 시점") 넘겼다가 깨우기 엔진이 상한으로 버린 공시 · 뉴스는 "본 것"에서
//! 빼 다음 가져오기에서 다시 넘긴다(trigger 8차 [`pi_trigger::Trigger::take_returned_data`]).
//!
//! - 엔진은 전체 정지로 버린 것(F4 7b2) · 구독을 꺼 버린 것 · 실패한 깨우기의 것은 돌려주지 않는다 — 그것들은 그대로
//!   "본 것"으로 남는다(의도된 버림).
//! - 재무 · 거시 값 바뀜(`team/changes.rs`)은 "본 것"이 아니라 지난번 값으로 거르므로 여기서 다루지 않는다
//!   (돌려받아도 버린다 — 다음 값 바뀜에서 다시 넘긴다). 시연용 가짜 공시는 본 것 기억을 거치지 않는다.

use pi_trigger::DataKind;

use super::Poller;
use crate::team::seen::Seen;

impl Poller {
    /// 엔진이 돌려준 사건 가운데 공시 · 뉴스를 "본 것"에서 뺀다.
    pub(super) fn forget_returned(&mut self) {
        let returned = self.trigger.take_returned_data();
        if returned.is_empty() {
            return;
        }
        let store = self.crew.store.clone();
        let mut forgotten = 0_usize;
        for change in returned {
            let (DataKind::News | DataKind::Disclosure) = change.kind else {
                continue;
            };
            let Some(symbol) = change.symbol.as_ref() else {
                continue;
            };
            let key = Seen::key(&change.source, symbol.as_str());
            self.seen.forget(&store, &key, &change.id);
            forgotten += 1;
        }
        if forgotten > 0 {
            tracing::info!(
                forgotten,
                "깨우기 상한으로 버려진 자료를 본 것에서 빼 다음 가져오기에서 다시 넘긴다"
            );
        }
    }
}
