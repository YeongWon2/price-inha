//! 시연 모드의 가짜 AI 공급자. 바깥으로 아무것도 보내지 않는다.
//!
//! 요청이 어느 턴인지는 요청 글로 안다: 시스템 글의 `담당 id: <id>`로 담당을, 마지막 사용자 글의 머리로 턴을
//! 고른다(`[점수]` → 점수, `판단을 JSON 객체 하나로` → 팀장 판단, `[주문 안]` → 주문 안, `[하루 판단]` → 하루 운용 판단,
//! `[복기]` → 복기, 그 밖 → 논의 글).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use pi_agent::Role;
use pi_ai::{
    AiError, AiProvider, CompletionRequest, CompletionResponse, Role as Speaker, StopReason, Usage,
};
use serde_json::json;
use tokio::sync::Notify;

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
    /// (25차 3번) 답마다 두는 뜸.
    pause: Duration,
    /// 서버를 끄는 중이면 참: 기다리던 뜸을 바로 끝내고 이 뒤로 뜸을 두지 않는다.
    closed: AtomicBool,
    /// 끄기를 기다리던 뜸에 알린다.
    closing: Notify,
    /// (51차) 턴 → 앞으로 공급자 장애로 답할 남은 횟수.
    failing: Mutex<HashMap<DemoTurn, u32>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl DemoAi {
    /// 기본 답으로 만든다.
    pub fn new() -> DemoAi {
        DemoAi::default()
    }

    /// (25차 3번) 답마다 `pause`만큼 뜸을 두는 가짜 AI. 뜸은 실제 시간(벽시계)으로 기다린다 — 담당들이
    /// 미팅룸에서 차례로 말하는 모습이 사람에게 보이게 하는 것이다. 실행 파일의 `--demo` 조립만 쓴다
    /// ([`crate::RunAssembly::demo_ai_pause`]). [`DemoAi::new`]는 뜸이 없다(`Duration::ZERO`).
    pub fn with_pause(pause: Duration) -> DemoAi {
        DemoAi {
            pause,
            ..DemoAi::default()
        }
    }

    /// (25차 3번) 답마다 두는 뜸. [`DemoAi::new`]는 `Duration::ZERO`.
    pub fn pause(&self) -> Duration {
        self.pause
    }

    /// 서버를 끈다: 기다리던 뜸을 바로 끝내고 이 뒤로는 뜸 없이 답한다(끄기가 뜸을 기다리지 않게).
    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.closing.notify_waiters();
    }

    /// 뜸을 둔다(뜸이 0이거나 끄는 중이면 곧바로 끝난다).
    async fn rest(&self) {
        if self.pause.is_zero() || self.closed.load(Ordering::SeqCst) {
            return;
        }
        let closing = self.closing.notified();
        tokio::pin!(closing);
        closing.as_mut().enable();
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        tokio::select! {
            () = tokio::time::sleep(self.pause) => {}
            () = closing => {}
        }
    }

    /// 그 턴의 답을 정한다. 다시 정하면 바뀐다. 이 뒤로 그 턴은 이 글을 답한다.
    pub fn script(&self, turn: DemoTurn, reply: impl Into<String>) {
        lock(&self.scripts).insert(turn, reply.into());
    }

    /// (51차) 이 뒤로 그 턴의 요청 `times`번을 공급자 장애(503)로 답한다(다시 시도 · 오류 표시 시험용, E51).
    /// 받은 글은 그대로 남긴다([`DemoAi::prompts`]). 0이면 지운다.
    pub fn fail_next(&self, turn: DemoTurn, times: u32) {
        let mut failing = lock(&self.failing);
        if times == 0 {
            failing.remove(&turn);
        } else {
            failing.insert(turn, times);
        }
    }

    /// 그 턴을 이번에 실패로 답할 차례인가(남은 횟수를 하나 쓴다).
    fn take_failure(&self, turn: DemoTurn) -> bool {
        let mut failing = lock(&self.failing);
        match failing.get_mut(&turn) {
            Some(left) if *left > 0 => {
                *left -= 1;
                if *left == 0 {
                    failing.remove(&turn);
                }
                true
            }
            _ => false,
        }
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
    } else if last.contains("[주문 안]") || last.contains("[팔기 주문 안]") {
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
        DemoTurn::Chat(role) => chat_reply(role, conversation),
    }
}

/// (다듬기 E 4번) 되받는 물음의 가장 긴 글자 수(넘으면 줄이고 "…").
const ECHO_MAX_CHARS: usize = 40;

/// 사무실 표시 이름(`crate::OFFICE_ROSTER`와 같다).
fn display_name(role: Role) -> &'static str {
    match role {
        Role::Scout => "뉴스 공시 시세",
        Role::Strategy => "전략",
        Role::Risk => "리스크",
        Role::Review => "복기",
        Role::Trader => "매매",
        Role::Leader => "팀장",
    }
}

/// (다듬기 E 4번) 사무실 채팅의 시연 답: 표시 이름 · 물음 한 줄 되받기 · "(시연)". 한 줄이다.
fn chat_reply(role: Role, conversation: &str) -> String {
    let name = display_name(role);
    let asked = conversation
        .rsplit(CHAT_HEAD)
        .next()
        .filter(|_| conversation.contains(CHAT_HEAD))
        .and_then(|rest| rest.lines().next())
        .map(str::trim)
        .unwrap_or_default();
    let mut echo: String = asked.chars().take(ECHO_MAX_CHARS).collect();
    if asked.chars().count() > ECHO_MAX_CHARS {
        echo.push('…');
    }
    if echo.is_empty() {
        format!(
            "{name}입니다. 시연이라 실제 자료를 보지 않고 답합니다 — 키를 등록하면 실제로 살펴봅니다. (시연)"
        )
    } else {
        format!(
            "{name}입니다. '{echo}'에 대해서는 시연이라 실제 자료를 보지 않았습니다 — 키를 등록하면 실제로 살펴보고 답합니다. (시연)"
        )
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
        let turn = turn_of(&request, &last);
        if self.take_failure(turn) {
            return Err(AiError::ProviderUnavailable { status: 503 });
        }
        let text = self.reply(turn, &conversation);
        self.rest().await;
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
