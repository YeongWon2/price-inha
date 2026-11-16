//! 턴 실행기(만드는 것 2번).
//!
//! 고리(원본 Hermes와 같은 순서): 반복 상한 확인 → 요청 조립(가리기) → 호출(시간 제한) →
//! 답 해석 → 행동이 있으면 **먼저 저장하고** 도구를 부른 뒤 결과를 붙여 다시, 없으면 최종 답 저장.
//!
//! - 반복 상한([`RoleConfig::max_iterations`])에 닿으면 [`TurnFailure::IterationLimit`].
//! - 공급자 호출 하나가 [`TimeLimits::idle`]을 넘으면 [`TurnFailure::IdleTimeout`],
//!   턴 전체가 [`TimeLimits::total`]을 넘으면 [`TurnFailure::TotalTimeout`].
//! - 모르는 도구만 부른 답: 호출마다 "그런 도구 없음" 결과를 붙여 다시 묻는다.
//!   연달아 [`MAX_UNKNOWN_TOOL_REPLIES`]번이면 [`TurnFailure::UnknownTools`].
//!   아는 도구와 섞이면 모르는 것에만 오류 결과를 주고 아는 것은 부르며, 세기를 0으로 되돌린다.
//! - 깨진 JSON 행동: 연달아 [`MAX_MALFORMED_REPLIES`]번이면 [`TurnFailure::MalformedAction`].
//! - 빈 답: 연달아 [`MAX_EMPTY_REPLIES`]번이면 [`TurnFailure::EmptyReply`].
//! - 도구를 부르기 전에 assistant 메시지(도구 호출 포함)를 세션에 저장한다(persist-before-execute).
//!   저장하지 못하면 도구를 부르지 않고 [`TurnFailure::Persistence`].
//! - 공급자 오류는 [`TurnFailure::Provider`].
//! - 어느 실패든 [`Error::RoleUnavailable`](E6)로 돌려준다.
//! - 공급자로 나가는 시스템 글 · 대화 글은 모두 [`Redactor`]를 거친다(E38).

use std::sync::Arc;

use pi_ai::{AiProvider, CompletionRequest, Message as AiMessage, Redactor, Role as AiRole};
use pi_store::{AgentMessage, MessageRole, NewMessage, SessionId, Store, ToolCall};

use crate::action::{ActionCall, Reply, parse_reply};
use crate::error::{Error, TurnFailure};
use crate::ports::{Clock, Tools};
use crate::prompt;
use crate::role::{RoleConfig, TimeLimits};

/// 모르는 도구만 부른 답의 연속 상한(원본 그대로).
pub const MAX_UNKNOWN_TOOL_REPLIES: u32 = 3;
/// 깨진 JSON 행동 답의 연속 상한(원본 그대로).
pub const MAX_MALFORMED_REPLIES: u32 = 3;
/// 빈 답의 연속 상한(원본 그대로).
pub const MAX_EMPTY_REPLIES: u32 = 3;

/// 실행기가 받는 포트 묶음.
#[derive(Clone)]
pub struct Ports {
    /// AI 공급자.
    pub ai: Arc<dyn AiProvider>,
    /// 도구.
    pub tools: Arc<dyn Tools>,
    /// 저장소(세션 · 메시지 · 방 로그 · 매매).
    pub store: Store,
    /// 가리기(E38).
    pub redactor: Arc<dyn Redactor>,
    /// 시계.
    pub clock: Arc<dyn Clock>,
}

/// 끝난 턴.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnOutcome {
    /// 최종 답 글.
    pub text: String,
    /// 공급자를 부른 횟수.
    pub iterations: u32,
    /// 이 턴이 쓴 세션.
    pub session: SessionId,
}

/// 턴 실행기. 담당 6명이 모두 같은 고리를 쓴다.
#[derive(Clone)]
pub struct TurnRunner {
    ports: Ports,
}

impl TurnRunner {
    /// 포트로 만든다.
    pub fn new(ports: Ports) -> TurnRunner {
        TurnRunner { ports }
    }

    /// 담당 한 명의 턴 하나를 돈다.
    ///
    /// - 세션은 `store.open_session(<담당 id>, session_title, 지금)`으로 찾거나 연다.
    /// - 받은 `input`을 user 메시지로 저장하고, 세션의 앞선 메시지를 대화로 이어 보낸다.
    /// - 끝나면 최종 답을 assistant 메시지로 저장하고 [`TurnOutcome`]을 준다.
    pub async fn run_turn(
        &self,
        config: &RoleConfig,
        limits: TimeLimits,
        session_title: &str,
        input: &str,
    ) -> Result<TurnOutcome, Error> {
        let fail = |cause| Error::RoleUnavailable {
            role: config.role,
            cause,
        };
        let (session, history) = self
            .start(config, session_title, input)
            .map_err(|_| fail(TurnFailure::Persistence))?;
        let tools = self.ports.tools.names(config.role);
        let mut turn = Turn {
            ports: &self.ports,
            config,
            session,
            system: prompt::system(config, &tools),
            tools,
            messages: history,
            iterations: 0,
            streaks: Streaks::default(),
        };
        match tokio::time::timeout(limits.total, turn.drive(limits)).await {
            Err(_) => Err(fail(TurnFailure::TotalTimeout)),
            Ok(Err(cause)) => Err(fail(cause)),
            Ok(Ok(text)) => Ok(TurnOutcome {
                text,
                iterations: turn.iterations,
                session,
            }),
        }
    }

    /// 세션을 열고 앞선 대화를 읽은 뒤 받은 글을 user 메시지로 저장한다.
    fn start(
        &self,
        config: &RoleConfig,
        title: &str,
        input: &str,
    ) -> Result<(SessionId, Vec<AiMessage>), pi_store::StoreError> {
        let store = &self.ports.store;
        let now = self.ports.clock.now();
        let session = store.open_session(config.role.id(), title, now)?.id;
        let mut history = to_conversation(&store.session_messages(session)?);
        store.append_messages(
            session,
            &[new_message(MessageRole::User, input.to_string(), now)],
        )?;
        push(&mut history, AiRole::User, input.to_string());
        Ok((session, history))
    }
}

/// 연달아 온 잘못된 답의 세기.
#[derive(Default)]
struct Streaks {
    unknown: u32,
    malformed: u32,
    empty: u32,
}

/// 도는 중인 턴 하나.
struct Turn<'a> {
    ports: &'a Ports,
    config: &'a RoleConfig,
    session: SessionId,
    system: String,
    tools: Vec<String>,
    messages: Vec<AiMessage>,
    iterations: u32,
    streaks: Streaks,
}

impl Turn<'_> {
    /// 고리를 돈다. 최종 답 글을 준다.
    async fn drive(&mut self, limits: TimeLimits) -> Result<String, TurnFailure> {
        loop {
            if self.iterations >= self.config.max_iterations {
                return Err(TurnFailure::IterationLimit);
            }
            self.iterations += 1;
            let reply = self.ask(limits).await?;
            match parse_reply(&reply) {
                Reply::Final(text) => {
                    self.persist(vec![assistant(text.clone(), Vec::new(), self.now())])?;
                    return Ok(text);
                }
                Reply::Actions(calls) => self.act(reply, calls).await?,
                Reply::Malformed => {
                    self.streaks = Streaks {
                        malformed: self.streaks.malformed + 1,
                        ..Streaks::default()
                    };
                    if self.streaks.malformed >= MAX_MALFORMED_REPLIES {
                        return Err(TurnFailure::MalformedAction);
                    }
                    push(&mut self.messages, AiRole::Assistant, reply);
                    push(
                        &mut self.messages,
                        AiRole::User,
                        prompt::MALFORMED_NUDGE.into(),
                    );
                }
                Reply::Empty => {
                    self.streaks = Streaks {
                        empty: self.streaks.empty + 1,
                        ..Streaks::default()
                    };
                    if self.streaks.empty >= MAX_EMPTY_REPLIES {
                        return Err(TurnFailure::EmptyReply);
                    }
                }
            }
        }
    }

    /// 공급자를 한 번 부른다(가리기 · 무응답 시간 제한).
    async fn ask(&self, limits: TimeLimits) -> Result<String, TurnFailure> {
        let redactor = &self.ports.redactor;
        let request = CompletionRequest {
            model: self.config.model.clone(),
            system: Some(redactor.redact(&self.system)),
            messages: self
                .messages
                .iter()
                .map(|m| AiMessage {
                    role: m.role,
                    content: redactor.redact(&m.content),
                })
                .collect(),
            max_tokens: self.config.max_tokens,
        };
        match tokio::time::timeout(limits.idle, self.ports.ai.complete(request)).await {
            Err(_) => Err(TurnFailure::IdleTimeout),
            Ok(Err(error)) => Err(TurnFailure::Provider(error)),
            Ok(Ok(response)) => Ok(response.text),
        }
    }

    /// 행동을 돈다: 먼저 저장하고, 아는 도구는 부르고 모르는 것에는 오류 결과를 준다.
    async fn act(&mut self, reply: String, calls: Vec<ActionCall>) -> Result<(), TurnFailure> {
        let now = self.now();
        let ids: Vec<String> = (0..calls.len())
            .map(|index| format!("call-{}-{}-{}", now.0, self.iterations, index))
            .collect();
        let tool_calls = calls
            .iter()
            .zip(&ids)
            .map(|(call, id)| ToolCall {
                id: id.clone(),
                name: call.name.clone(),
                arguments: call.args.to_string(),
            })
            .collect();
        self.persist(vec![assistant(reply.clone(), tool_calls, now)])?;
        let all_unknown = calls.iter().all(|call| !self.knows(call));
        let mut results = Vec::with_capacity(calls.len());
        let mut stored = Vec::with_capacity(calls.len());
        for (call, id) in calls.iter().zip(ids) {
            let content = if self.knows(call) {
                match self.ports.tools.call(self.config.role, call).await {
                    Ok(text) => text,
                    Err(error) => error.to_string(),
                }
            } else {
                prompt::unknown_tool(&call.name)
            };
            results.push(prompt::tool_result(&call.name, &content));
            stored.push(tool_message(id, &call.name, content, self.now()));
        }
        self.persist(stored)?;
        push(&mut self.messages, AiRole::Assistant, reply);
        push(&mut self.messages, AiRole::User, results.join("\n\n"));
        self.streaks = Streaks {
            unknown: if all_unknown {
                self.streaks.unknown + 1
            } else {
                0
            },
            ..Streaks::default()
        };
        if self.streaks.unknown >= MAX_UNKNOWN_TOOL_REPLIES {
            return Err(TurnFailure::UnknownTools);
        }
        Ok(())
    }

    fn knows(&self, call: &ActionCall) -> bool {
        self.tools.contains(&call.name)
    }

    fn now(&self) -> pi_store::UnixMillis {
        self.ports.clock.now()
    }

    fn persist(&self, messages: Vec<NewMessage>) -> Result<(), TurnFailure> {
        self.ports
            .store
            .append_messages(self.session, &messages)
            .map(|_| ())
            .map_err(|_| TurnFailure::Persistence)
    }
}

/// 저장된 메시지를 공급자 대화로 바꾼다. 도구 결과는 user 쪽 글이 되고,
/// 같은 쪽 글이 이어지면 하나로 묶는다.
fn to_conversation(stored: &[AgentMessage]) -> Vec<AiMessage> {
    let mut out = Vec::new();
    for message in stored {
        let content = message.content.clone().unwrap_or_default();
        match message.role {
            MessageRole::User => push(&mut out, AiRole::User, content),
            MessageRole::Assistant => push(&mut out, AiRole::Assistant, content),
            MessageRole::Tool => {
                let name = message.tool_name.as_deref().unwrap_or("");
                push(&mut out, AiRole::User, prompt::tool_result(name, &content));
            }
            MessageRole::System => {}
        }
    }
    out
}

/// 대화 끝에 글을 붙인다. 끝 글과 같은 쪽이면 그 글에 잇는다.
fn push(messages: &mut Vec<AiMessage>, role: AiRole, content: String) {
    match messages.last_mut() {
        Some(last) if last.role == role => {
            last.content.push_str("\n\n");
            last.content.push_str(&content);
        }
        _ => messages.push(AiMessage { role, content }),
    }
}

fn new_message(role: MessageRole, content: String, at: pi_store::UnixMillis) -> NewMessage {
    NewMessage {
        role,
        content: Some(content),
        api_content: None,
        tool_calls: Vec::new(),
        tool_call_id: None,
        tool_name: None,
        finish_reason: None,
        reasoning: None,
        at,
    }
}

fn assistant(content: String, tool_calls: Vec<ToolCall>, at: pi_store::UnixMillis) -> NewMessage {
    let finish = if tool_calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };
    NewMessage {
        tool_calls,
        finish_reason: Some(finish.to_string()),
        ..new_message(MessageRole::Assistant, content, at)
    }
}

fn tool_message(id: String, name: &str, content: String, at: pi_store::UnixMillis) -> NewMessage {
    NewMessage {
        tool_call_id: Some(id),
        tool_name: Some(name.to_string()),
        ..new_message(MessageRole::Tool, content, at)
    }
}
