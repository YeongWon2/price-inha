//! 바깥에서 끼우는 자리: 시계와 가리기.

use pi_store::UnixMillis;

/// 시계. 이벤트 시각 · 푸시 서명 시각 · 조용한 시간 판단에 쓴다. 테스트가 시각을 정한다.
/// 실제 시계는 조립하는 곳(price-inha)이 넣는다.
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> UnixMillis;
}

/// 가리기 자리. 기록 · 푸시로 나가는 글이 모두 이것을 거친다(E38).
pub trait Redactor: Send + Sync {
    /// 글에서 비밀 값을 가린 글을 돌려준다. 가릴 것이 없으면 그대로 돌려준다.
    fn redact(&self, text: &str) -> String;
}

/// 아무것도 가리지 않는다(시험 · 기본값).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRedaction;

impl Redactor for NoRedaction {
    fn redact(&self, text: &str) -> String {
        text.to_owned()
    }
}
