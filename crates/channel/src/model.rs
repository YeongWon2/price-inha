//! 채널이 주고받는 값.

use chrono::{DateTime, Utc};

/// 보낸 사람 ID. 텔레그램은 숫자 user ID를 글자로 둔다(`from.id`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SenderId(pub String);

/// 대화 ID. 텔레그램은 `chat.id`를 글자로 둔다. 보낼 곳([`crate::Channel::send`]의 `to`)이다.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChatId(pub String);

/// 보낸 글 하나를 가리키는 값.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MsgRef {
    /// 보낸 대화.
    pub chat: ChatId,
    /// 채널이 준 글 번호(텔레그램 `message_id`).
    pub message_id: i64,
}

/// 받은 글 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    /// 채널 이름(예: [`crate::TELEGRAM_CHANNEL`]).
    pub channel: String,
    /// 보낸 사람.
    pub sender: SenderId,
    /// 들어온 대화. 답은 이 대화로 돌아간다.
    pub chat: ChatId,
    /// 1:1 대화(DM)인가. 텔레그램은 `chat.type == "private"`일 때만 참이다.
    pub is_direct: bool,
    /// 글.
    pub text: String,
    /// 채널이 적은 보낸 시각(텔레그램 `date`, 초).
    pub at: DateTime<Utc>,
    /// 채널이 준 글 번호(텔레그램 `message_id`).
    pub message_id: i64,
}

/// 받기 자리. 다음 [`crate::Channel::poll`]에 그대로 넘긴다.
/// 텔레그램은 `getUpdates`의 `offset`(= 마지막 `update_id` + 1)이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cursor(pub i64);

/// [`crate::Channel::poll`] 한 번의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Polled {
    /// 받은 글(받은 차례대로). 글이 아닌 갱신(사진 · 편집 등)은 넣지 않는다.
    pub items: Vec<Inbound>,
    /// 다음 받기 자리. 받은 갱신이 없으면 넘긴 자리 그대로다.
    /// 글이 아닌 갱신도 자리는 넘긴다(같은 갱신을 다시 받지 않는다).
    pub next: Option<Cursor>,
}

/// 채널의 한도.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// 글 하나의 최대 글자 수(유니코드 글자 기준). 텔레그램은 [`crate::TELEGRAM_TEXT_MAX`].
    pub text_max: usize,
}
