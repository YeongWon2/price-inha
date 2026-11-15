//! 나가는 글의 가리기 자리(ai.md 7차 1, E38 · V47).
//!
//! 공급자로 나가는 글(Anthropic 요청의 시스템 글 · 대화 글, CLI의 표준 입력 · 시스템 글 파일)은
//! 보내기 전에 모두 [`Redactor`]를 거친다. 가리기 자체는 이 모듈이 하지 않는다:
//! 조립(`price-inha`)이 `pi-secrets`의 가리기를 이 자리에 끼운다.

/// 가리기 자리. 공급자로 나가는 글이 모두 이것을 거친다.
pub trait Redactor: Send + Sync {
    /// 글에서 비밀 값을 가린 글을 돌려준다. 가릴 것이 없으면 그대로 돌려준다.
    fn redact(&self, text: &str) -> String;
}

/// 아무것도 가리지 않는다(기본값). 가리기를 끼우지 않은 공급자가 쓴다.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRedaction;

impl Redactor for NoRedaction {
    fn redact(&self, text: &str) -> String {
        text.to_string()
    }
}
