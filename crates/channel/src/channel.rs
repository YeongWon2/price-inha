//! 채널 한 겹.

use async_trait::async_trait;

use crate::{ChatId, Cursor, Error, Limits, MsgRef, OutText, Polled};

/// 메신저 채널 하나(텔레그램 · 슬랙). 밖에서 들어오는 포트를 열지 않는다(long polling · Socket Mode).
///
/// 어느 메서드도 스스로 무한히 다시 하지 않는다. 429 · 연결 실패는 종류 있는 [`Error`]로 돌려주고,
/// 부르는 쪽이 정한 간격으로 다시 부른다.
#[async_trait]
pub trait Channel: Send + Sync {
    /// 채널 이름(예: `"telegram"`). [`crate::Inbound::channel`]과 같다.
    fn name(&self) -> &'static str;

    /// 토큰이 맞는지 확인하고 봇 이름(텔레그램 `getMe`의 `username`)을 돌려준다.
    /// 틀린 토큰은 [`Error::Unauthorized`].
    async fn check(&self) -> Result<String, Error>;

    /// 새 글을 받는다. `cursor`는 앞 [`Polled::next`](처음이면 `None`).
    async fn poll(&self, cursor: Option<Cursor>) -> Result<Polled, Error>;

    /// 가린 글을 `to`로 보낸다. 글이 [`Limits::text_max`]를 넘으면 [`crate::split_text`]로 나눠
    /// 차례대로 보내고, 보낸 조각의 글 번호를 차례대로 돌려준다. 중간 조각이 실패하면 그 오류를 돌려주고
    /// 나머지는 보내지 않는다.
    async fn send(&self, to: &ChatId, text: &OutText) -> Result<Vec<MsgRef>, Error>;

    /// 채널의 한도.
    fn limits(&self) -> Limits;
}
