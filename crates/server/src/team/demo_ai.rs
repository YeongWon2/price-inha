//! 시연 모드의 가짜 AI 공급자. 바깥으로 아무것도 보내지 않는다.
//!
//! 요청이 어느 턴인지는 요청 글로 안다: 시스템 글의 `담당 id: <id>`로 담당을, 마지막 사용자 글의 머리로 턴을
//! 고른다(`[점수]` → 점수, `판단을 JSON 객체 하나로` → 팀장 판단, `[주문 안]` → 주문 안, `[하루 판단]` → 하루 운용 판단,
//! `[복기]` → 복기, 그 밖 → 논의 글).

use async_trait::async_trait;
use pi_agent::Role;
use pi_ai::{AiError, AiProvider, CompletionRequest, CompletionResponse};

/// 가짜 AI가 답하는 턴.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DemoTurn {
    /// 팀 논의 글(그 담당). 기본: 뉴스 공시 시세 담당은 `@strategy @risk`를 부르는 글, 전략 · 리스크 담당은
    /// 짧은 의견, 나머지는 `(pass)`.
    Talk(Role),
    /// 점수 턴(그 담당). 기본 `{"score": 70, "reason": …}`.
    Score(Role),
    /// 팀장 판단. 기본: 대화에 `현재가 <정수>`가 있으면 그 값으로 사기
    /// (진입 = 현재가, 손절 = 현재가의 95%, 목표 = 현재가의 110%, 기대값 = 현재가의 5%, 모두 정수로 버림),
    /// 없으면 지켜보기.
    Verdict,
    /// 매매 담당의 주문 안. 기본 `{"side": "buy", "note": …}`.
    Draft,
    /// 팀장의 하루 운용 판단. 기본 `{"plan": "trade"}`.
    DayPlan,
    /// 복기 담당의 복기. 기본 `{"mistakes": [], "note": …}`.
    Review,
}

/// 시연 모드의 가짜 AI. 턴마다 정해진 답을 돌려준다. [`DemoAi::script`]로 답을 바꾼다.
#[derive(Debug, Default)]
pub struct DemoAi {
    _private: (),
}

impl DemoAi {
    /// 기본 답으로 만든다.
    pub fn new() -> DemoAi {
        todo!()
    }

    /// 그 턴의 답을 정한다. 다시 정하면 바뀐다. 이 뒤로 그 턴은 이 글을 답한다.
    pub fn script(&self, turn: DemoTurn, reply: impl Into<String>) {
        let _ = (turn, reply.into());
        todo!()
    }

    /// 받은 요청들의 마지막 사용자 글(받은 순서). 가리기를 거친 뒤의 글이다.
    pub fn prompts(&self) -> Vec<String> {
        todo!()
    }
}

#[async_trait]
impl AiProvider for DemoAi {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        let _ = request;
        todo!()
    }
}
