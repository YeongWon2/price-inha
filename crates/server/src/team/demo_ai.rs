//! 시연 모드의 가짜 AI 공급자. 바깥으로 아무것도 보내지 않는다.
//!
//! 요청이 어느 턴인지는 요청 글로 안다: 시스템 글의 `담당 id: <id>`로 담당을, 마지막 사용자 글의 머리로 턴을
//! 고른다(`[점수]` → 점수, `판단을 JSON 객체 하나로` → 팀장 판단, `[주문 안]` → 주문 안, `[하루 판단]` → 하루 운용 판단,
//! `[복기]` → 복기, 그 밖 → 논의 글).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;
use pi_agent::Role;
use pi_ai::{
    AiError, AiProvider, CompletionRequest, CompletionResponse, Role as Speaker, StopReason, Usage,
};
use serde_json::json;

/// 하루 운용 판단 턴의 머리.
pub(crate) const DAY_PLAN_HEAD: &str = "[하루 판단]";
/// 복기 턴의 머리.
pub(crate) const REVIEW_HEAD: &str = "[복기]";
/// (W5 3차) 사무실 채팅 턴의 머리.
pub(crate) const CHAT_HEAD: &str = "[사무실 채팅]";

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
    /// (W5 3차) 사무실 채팅에서 그 담당에게 물은 답(V14). 기본: 그 담당 이름이 들어간 짧은 답.
    Chat(Role),
}

/// 시연 모드의 가짜 AI. 턴마다 정해진 답을 돌려준다. [`DemoAi::script`]로 답을 바꾼다.
#[derive(Debug, Default)]
pub struct DemoAi {
    scripts: Mutex<HashMap<DemoTurn, String>>,
    prompts: Mutex<Vec<String>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DemoAi {
    /// 기본 답으로 만든다.
    pub fn new() -> DemoAi {
        DemoAi::default()
    }

    /// 그 턴의 답을 정한다. 다시 정하면 바뀐다. 이 뒤로 그 턴은 이 글을 답한다.
    pub fn script(&self, turn: DemoTurn, reply: impl Into<String>) {
        lock(&self.scripts).insert(turn, reply.into());
    }

    /// 받은 요청들의 마지막 사용자 글(받은 순서). 가리기를 거친 뒤의 글이다.
    pub fn prompts(&self) -> Vec<String> {
        lock(&self.prompts).clone()
    }

    /// 그 턴의 답: 정해 둔 것, 없으면 기본 답.
    fn reply(&self, turn: DemoTurn, conversation: &str) -> String {
        if let Some(script) = lock(&self.scripts).get(&turn) {
            return script.clone();
        }
        default_reply(turn, conversation)
    }
}

/// 요청이 어느 턴인지.
fn turn_of(request: &CompletionRequest, last: &str) -> DemoTurn {
    let role = request
        .system
        .as_deref()
        .and_then(|system| system.split("담당 id: ").nth(1))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(Role::from_id)
        .unwrap_or(Role::Leader);
    if last.contains(CHAT_HEAD) {
        DemoTurn::Chat(role)
    } else if last.contains(DAY_PLAN_HEAD) {
        DemoTurn::DayPlan
    } else if last.contains(REVIEW_HEAD) {
        DemoTurn::Review
    } else if last.contains("[점수]") {
        DemoTurn::Score(role)
    } else if last.contains("판단을 JSON 객체 하나로") {
        DemoTurn::Verdict
    } else if last.contains("[주문 안]") {
        DemoTurn::Draft
    } else {
        DemoTurn::Talk(role)
    }
}

/// 대화에서 `현재가 <정수>`를 찾는다(마지막 것).
fn price_in(conversation: &str) -> Option<i64> {
    conversation
        .rsplit("현재가 ")
        .next()
        .filter(|_| conversation.contains("현재가 "))
        .and_then(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
}

fn default_reply(turn: DemoTurn, conversation: &str) -> String {
    match turn {
        DemoTurn::Talk(Role::Scout) => "새 자료를 확인했다. @strategy @risk 의견을 부탁한다".into(),
        DemoTurn::Talk(Role::Strategy) => "전략상 진입할 만하다.".into(),
        DemoTurn::Talk(Role::Risk) => "손절 기준을 지키면 감당할 만하다.".into(),
        DemoTurn::Talk(_) => "(pass)".into(),
        DemoTurn::Score(_) => json!({"score": 70, "reason": "시연 점수"}).to_string(),
        DemoTurn::Verdict => match price_in(conversation) {
            Some(price) => json!({
                "decision": "buy", "hypothesis": "시연 가설", "trigger": "시연 계기",
                "entry": price, "stop": price * 95 / 100, "target": price * 110 / 100,
                "expected_value": price * 5 / 100
            })
            .to_string(),
            None => {
                json!({"decision": "watch", "reason": "현재가를 모른다", "revisit": "시세가 오면"})
                    .to_string()
            }
        },
        DemoTurn::Draft => json!({"side": "buy", "note": "시연 주문 안"}).to_string(),
        DemoTurn::DayPlan => json!({"plan": "trade"}).to_string(),
        DemoTurn::Review => json!({"mistakes": [], "note": "시연 복기"}).to_string(),
        DemoTurn::Chat(role) => {
            format!("{} 담당입니다. 지금은 따로 알릴 일이 없습니다.", role.id())
        }
    }
}

#[async_trait]
impl AiProvider for DemoAi {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        let last = request
            .messages
            .iter()
            .rev()
            .find(|message| message.role == Speaker::User)
            .map(|message| message.content.clone())
            .unwrap_or_default();
        lock(&self.prompts).push(last.clone());
        let conversation: String = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let text = self.reply(turn_of(&request, &last), &conversation);
        Ok(CompletionResponse {
            text,
            stop_reason: StopReason::EndTurn,
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
            },
        })
    }
}
