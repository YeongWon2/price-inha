//! (백로그 Q) 깨우기 턴의 세션을 닫는다.
//!
//! 뉴스 공시 시세 담당의 깨우기 턴은 깨울 때마다 새 제목(`wake-<종목>-<시각>` · `wake-macro-<시각>`)으로 세션을
//! 연다. 다시 이어 쓸 일이 없는데 열린 채로 두면 열린 세션이 끝없이 쌓인다. 그래서 턴이 끝나면(답했든 못 했든)
//! 그 세션을 끝낸다(끝낸 시각과 까닭 [`WAKE_DONE`]이 남는다 — 대화 기록은 지우지 않는다).

use pi_agent::Role;
use pi_store::SessionId;

use super::Crew;

/// 깨우기 턴이 끝나 세션을 닫은 까닭(원본의 `end_reason`과 같은 자리).
pub(super) const WAKE_DONE: &str = "wake_done";

impl Crew {
    /// 깨우기 턴이 쓴 세션을 닫는다. 세션 번호를 모르면(턴이 실패) 그 제목의 열린 세션을 찾아 닫는다.
    /// 닫지 못하면 기록만 한다(깨우기 결과는 그대로다).
    pub(super) fn close_wake_session(&self, title: &str, session: Option<SessionId>) {
        let at = pi_store::UnixMillis(self.clock.now().timestamp_millis());
        let found = match session {
            Some(id) => Some(id),
            None => self
                .store
                .sessions_of(Role::Scout.id())
                .ok()
                .and_then(|all| {
                    all.into_iter()
                        .rev()
                        .find(|s| s.title == title && s.ended_at.is_none())
                })
                .map(|s| s.id),
        };
        let Some(id) = found else {
            return;
        };
        match self.store.end_session(id, WAKE_DONE, at) {
            Ok(_) | Err(pi_store::StoreError::SessionEnded(_)) => {}
            Err(error) => tracing::warn!(%error, "깨우기 세션을 닫지 못했다"),
        }
    }
}
