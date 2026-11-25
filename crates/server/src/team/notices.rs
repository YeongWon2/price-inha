//! (33차 3번) 사무실 전체 공지와 팀을 잇는 곳(C19 · F11 · E45 · N14 · 결정 17 · 19).
//!
//! - 공지 확인 답 · 담당끼리 이어 말하기는 하루 AI 호출 상한에 든다. 다만 팀장 예비분(상한의 10% 올림, 최소 2번)은
//!   쓰지 않는다(E45). 상한이 없으면 세지 않는다(논의와 같다).
//! - 공지는 그 뒤 열리는 논의의 담당 자료로 넘어간다. 논의를 여는 시각까지 보낸 공지만 넘긴다(결정 19 — 보낸 시각이
//!   판단 시각보다 늦은 공지가 판단에 붙으면 저장소가 판단을 거부한다).

use pi_agent::{DiscussionOptions, NoticeNote};
use pi_store::UnixMillis;

use super::Crew;

/// 팀장 예비분의 가장 작은 값(N14).
const LEADER_RESERVE_MIN: u32 = 2;

/// 논의 자료로 넘기는 공지를 고르는 기간(밀리초): 논의를 여는 시각에서 7일 전까지.
const NOTICE_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// 논의 자료로 넘기는 공지의 가장 많은 수(최근 것부터).
const NOTICE_NOTES_MAX: usize = 20;

/// 하루 상한에서 팀장만 쓰는 몫(N14): 상한의 10%(올림), 최소 2번.
pub(super) fn leader_reserve(limit: u32) -> u32 {
    limit.div_ceil(10).max(LEADER_RESERVE_MIN)
}

impl Crew {
    /// 공지 확인 답 · 이어 말하기 한 번을 하루 상한에서 잡는다. 팀장 예비분을 남기고 남은 몫이 없으면 거짓.
    /// 상한이 꺼져 있으면 늘 참이다. (51차) 그래도 그날 쓴 수는 센다(잔량 바의 쓴 수).
    pub(crate) fn take_spare_ai_call(&self) -> bool {
        let limit = self
            .wiring
            .ai_daily_limit
            .or_else(|| self.desk.saved_ai_limit());
        let Some(limit) = limit else {
            self.ai_cap.take_within(self.clock.now(), u32::MAX);
            return true;
        };
        let spare = limit.saturating_sub(leader_reserve(limit));
        self.ai_cap.take_within(self.clock.now(), spare)
    }

    /// 논의 옵션: 논의를 여는 지금까지 보낸 공지(최근 [`NOTICE_WINDOW_MS`] 안, 많아야 [`NOTICE_NOTES_MAX`]개)를
    /// 담당 자료로 넘긴다. 공지를 읽지 못하면 공지 없이 연다.
    pub(super) fn discussion_options(&self) -> DiscussionOptions {
        let now = self.clock.now().timestamp_millis();
        let from = UnixMillis(now.saturating_sub(NOTICE_WINDOW_MS));
        let notices = match self.store.notices_between(from, UnixMillis(now)) {
            Ok(notices) => notices,
            Err(error) => {
                tracing::warn!(%error, "공지를 읽지 못해 공지 없이 논의를 엽니다");
                Vec::new()
            }
        };
        let skip = notices.len().saturating_sub(NOTICE_NOTES_MAX);
        let notes = notices
            .into_iter()
            .skip(skip)
            .map(|notice| NoticeNote {
                text: notice.text,
                sent_at: notice.sent_at,
            })
            .collect();
        DiscussionOptions::default().with_notices(notes)
    }
}

#[cfg(test)]
mod tests {
    use super::leader_reserve;

    #[test]
    fn 팀장_예비분은_상한의_10퍼센트_올림이고_최소_2번이다() {
        assert_eq!(leader_reserve(2), 2);
        assert_eq!(leader_reserve(20), 2);
        assert_eq!(leader_reserve(21), 3);
        assert_eq!(leader_reserve(100), 10);
    }
}
