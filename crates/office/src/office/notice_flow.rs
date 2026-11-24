//! 공지 나눠 주기(6차). `Office`의 안쪽 칸을 그대로 쓰는 `impl Office` 조각이다.

use std::sync::Arc;

use pi_store::Store;

use super::Office;
use crate::error::Error;
use crate::notice::NoticePost;
use crate::port::{AiBudget, Redactor};

impl Office {
    /// 공지 저장소와 가리기를 끼운다. 끼우지 않으면 [`Office::post_notice`]는 [`Error::NoticeUnavailable`].
    pub fn with_notices(self, store: Store, redactor: Arc<dyn Redactor>) -> Office {
        let _ = (store, redactor);
        todo!()
    }

    /// 하루 AI 호출 상한 판정 포트를 끼운다. 끼우지 않으면 늘 허용한다.
    pub fn with_ai_budget(self, budget: Arc<dyn AiBudget>) -> Office {
        let _ = budget;
        todo!()
    }

    /// 공지를 받는다(C19 · 결정 17).
    ///
    /// 1. 글이 0자이거나 [`pi_store::NOTICE_TEXT_MAX_CHARS`]자(글자 수)보다 길면 [`Error::InvalidNotice`]이고
    ///    아무것도 남기지 않는다(저장 · 이벤트 없음).
    /// 2. 글을 가린 뒤 저장소에 공지를 만든다. 담당 여섯([`crate::Agent::ALL`], 이름은 [`crate::Agent::id`])이 모두 기다림.
    /// 3. `office.notice` `{"notice", "text", "at"}`(가린 글)를 낸다.
    /// 4. 담당마다 [`AiBudget::try_take`]로 물어 허용이면 확인 답 응답 객체(queued, 팀 방, `notice` 붙음)를
    ///    대기열에 넣고 `office.chat`을 낸다. 상한이면 저장소 확인을 "상한으로 못 함"으로 바꾸고
    ///    `office.notice_ack` `{"notice", "agent", "state": "limit"}`를 낸다(E45).
    /// 5. 확인 답이 끝나면 [`Office::complete`] · [`Office::fail`]이 "답함" · "실패"로 한 번만 바꾼다.
    ///
    /// 공지는 규칙 · 템플릿 · 승인 · 팀 상태를 바꾸지 않는다(E46).
    pub async fn post_notice(
        &self,
        text: &str,
        source_message_id: &str,
    ) -> Result<NoticePost, Error> {
        let _ = (text, source_message_id);
        todo!()
    }
}
