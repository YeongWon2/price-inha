//! 에이전트 세션과 메시지(10차 1번). 원본 근거: `docs/research/agent-runtime-deep.md` 1장 · 5장.
//!
//! - 세션은 담당(`agent`)마다 나뉜다. 원본(Hermes)은 프로필마다 DB 파일을 따로 두었지만
//!   가격인하는 파일 하나(D6)에 `agent` 칸으로 나눈다(조사 문서 5장 "줄여서").
//! - 메시지 한 줄 = OpenAI 형식 메시지 하나. `content`(화면 · 검색용)와 `api_content`(실제로 보낸 글이
//!   달랐을 때만)를 나눠 둔다.
//! - 메시지는 여러 개를 **한 묶음으로** 쓴다: 도구를 부르기 전에 assistant의 도구 호출 줄을 먼저
//!   저장하고(persist-before-execute), 저장이 실패하면 도구를 돌리지 않는다. 묶음 안에 하나라도
//!   거부되면 아무것도 쓰지 않는다.

use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};

use crate::model::UnixMillis;
use crate::{Store, StoreError};

/// 세션 번호. 저장소가 붙인다(1부터).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub i64);

/// 메시지 번호. 저장소가 붙인다. 모든 세션을 통틀어 1부터 빠짐없이 커진다(원본의 `message_uid`처럼
/// 메시지마다 한 번 매기는 영구 번호다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(pub i64);

/// 저장된 세션.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    /// 번호.
    pub id: SessionId,
    /// 담당 이름(누구의 세션인지).
    pub agent: String,
    /// 제목. 같은 담당의 열린 세션 가운데 제목으로 찾아 이어 쓴다(원본: 방 × 프로필마다 하나).
    pub title: String,
    /// 연 시각.
    pub started_at: UnixMillis,
    /// 끝낸 시각. 열려 있으면 없다.
    pub ended_at: Option<UnixMillis>,
    /// 끝낸 이유(원본의 `end_reason`, 예: `session_reset`). 열려 있으면 없다.
    pub end_reason: Option<String>,
    /// 이 세션에 쓴 메시지 수. 쓸 때마다 커진다.
    pub message_count: u64,
    /// 이 세션의 메시지에 든 도구 호출 수. 쓸 때마다 커진다.
    pub tool_call_count: u64,
}

/// 메시지를 보낸 쪽(OpenAI 형식의 `role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageRole {
    /// 시스템.
    System,
    /// 사용자.
    User,
    /// 모델.
    Assistant,
    /// 도구 결과.
    Tool,
}

/// 모델이 부른 도구 하나(assistant 메시지의 `tool_calls` 한 칸).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// 호출 id. 도구 결과 메시지의 `tool_call_id`와 짝이 된다.
    pub id: String,
    /// 도구 이름.
    pub name: String,
    /// 인자. 모델이 준 글자 그대로(JSON이 아니어도) 저장한다.
    pub arguments: String,
}

/// 쓸 메시지 하나.
///
/// 규칙(어기면 [`StoreError::InvalidMessage`]):
/// - `Tool`은 `tool_call_id`가 있어야 한다.
/// - `tool_calls`는 `Assistant`만 가질 수 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessage {
    /// 보낸 쪽.
    pub role: MessageRole,
    /// 화면 · 검색용 글. 없을 수 있다(도구 호출만 있는 assistant 등).
    pub content: Option<String>,
    /// 실제로 API에 보낸 글이 `content`와 다를 때만 둔다. 바이트 그대로 저장한다.
    pub api_content: Option<String>,
    /// 도구 호출들(assistant만). 준 순서대로 저장한다.
    pub tool_calls: Vec<ToolCall>,
    /// 도구 결과가 어느 호출의 것인지(tool만).
    pub tool_call_id: Option<String>,
    /// 도구 결과를 낸 도구 이름(tool).
    pub tool_name: Option<String>,
    /// 모델이 멈춘 이유(`stop` · `tool_calls` · `length` 등). 받은 그대로.
    pub finish_reason: Option<String>,
    /// 추론 글(assistant). 받은 그대로.
    pub reasoning: Option<String>,
    /// 쓴 시각.
    pub at: UnixMillis,
}

/// 저장된 메시지.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessage {
    /// 번호.
    pub id: MessageId,
    /// 세션.
    pub session: SessionId,
    /// 보낸 쪽.
    pub role: MessageRole,
    /// 화면 · 검색용 글.
    pub content: Option<String>,
    /// 실제로 보낸 글(달랐을 때만).
    pub api_content: Option<String>,
    /// 도구 호출들.
    pub tool_calls: Vec<ToolCall>,
    /// 도구 결과의 호출 id.
    pub tool_call_id: Option<String>,
    /// 도구 이름.
    pub tool_name: Option<String>,
    /// 멈춘 이유.
    pub finish_reason: Option<String>,
    /// 추론 글.
    pub reasoning: Option<String>,
    /// 쓴 시각.
    pub at: UnixMillis,
}

impl Store {
    /// 담당 `agent`의 열린 세션 가운데 제목이 `title`인 것을 준다. 없으면 새로 연다(연 시각 `at`).
    ///
    /// - 같은 담당 · 같은 제목으로 다시 부르면 같은 세션이다(끝내기 전까지). `at`은 새로 열 때만 쓴다.
    /// - 끝낸 세션은 다시 찾지 않는다: 같은 제목으로 부르면 새 세션이 열린다(원본의 `session_reset`).
    /// - 담당이 다르면 제목이 같아도 다른 세션이다.
    pub fn open_session(
        &self,
        agent: &str,
        title: &str,
        at: UnixMillis,
    ) -> Result<AgentSession, StoreError> {
        self.write(|conn| {
            let found = conn
                .query_row(
                    &format!(
                        "{SESSION_SELECT} WHERE agent = ?1 AND title = ?2 AND ended_at IS NULL"
                    ),
                    params![agent, title],
                    session_from,
                )
                .optional()?;
            if let Some(session) = found {
                return session;
            }
            conn.execute(
                "INSERT INTO agent_sessions (agent, title, started_at) VALUES (?1, ?2, ?3)",
                params![agent, title, at.0],
            )?;
            load_session(conn, SessionId(conn.last_insert_rowid()))
        })
    }

    /// 세션 한 건. 없으면 [`StoreError::SessionNotFound`].
    pub fn get_session(&self, id: SessionId) -> Result<AgentSession, StoreError> {
        self.read(|conn| load_session(conn, id))
    }

    /// 그 담당의 세션 전체(끝낸 것 포함). 번호 순서다.
    pub fn sessions_of(&self, agent: &str) -> Result<Vec<AgentSession>, StoreError> {
        self.read(|conn| {
            let mut stmt =
                conn.prepare(&format!("{SESSION_SELECT} WHERE agent = ?1 ORDER BY id"))?;
            let rows = stmt.query_map([agent], session_from)?;
            rows.map(|row| row?).collect()
        })
    }

    /// 세션을 끝낸다(끝낸 시각과 이유를 남긴다).
    ///
    /// - 없으면 [`StoreError::SessionNotFound`].
    /// - 이미 끝났으면 [`StoreError::SessionEnded`]이고 그대로다.
    pub fn end_session(
        &self,
        id: SessionId,
        reason: &str,
        at: UnixMillis,
    ) -> Result<AgentSession, StoreError> {
        self.write(|conn| {
            let session = load_session(conn, id)?;
            if session.ended_at.is_some() {
                return Err(StoreError::SessionEnded(id));
            }
            conn.execute(
                "UPDATE agent_sessions SET ended_at = ?2, end_reason = ?3 WHERE id = ?1",
                params![id.0, at.0, reason],
            )?;
            load_session(conn, id)
        })
    }

    /// 메시지 여러 개를 **한 묶음으로** 쓰고 저장된 모양을 준 순서대로 돌려준다.
    ///
    /// - 묶음 전체가 되거나 하나도 안 된다. 하나라도 규칙([`NewMessage`])을 어기면
    ///   [`StoreError::InvalidMessage`](`index`는 처음 어긴 것의 자리)이고 아무것도 쓰지 않는다
    ///   (번호와 세기 값도 그대로다).
    /// - 세션이 없으면 [`StoreError::SessionNotFound`], 끝났으면 [`StoreError::SessionEnded`].
    /// - 되면 세션의 `message_count`는 쓴 수만큼, `tool_call_count`는 쓴 도구 호출 수만큼 커진다.
    /// - 짝 없는 도구 호출(결과가 아직 없는 것)도 그대로 받는다: 도구를 돌리기 전에 먼저 쓰는 것이
    ///   이 API의 목적이다. 짝 맞추기는 읽는 쪽이 읽은 사본에서 한다(저장본은 고치지 않는다).
    /// - 빈 묶음은 아무것도 쓰지 않고 빈 목록이다.
    pub fn append_messages(
        &self,
        session: SessionId,
        messages: &[NewMessage],
    ) -> Result<Vec<AgentMessage>, StoreError> {
        self.write(|conn| {
            let current = load_session(conn, session)?;
            if current.ended_at.is_some() {
                return Err(StoreError::SessionEnded(session));
            }
            if let Some(index) = messages.iter().position(|m| !is_valid(m)) {
                return Err(StoreError::InvalidMessage { index });
            }
            let mut written = Vec::with_capacity(messages.len());
            let mut tool_calls: i64 = 0;
            for message in messages {
                written.push(insert_message(conn, session, message)?);
                tool_calls += i64::try_from(message.tool_calls.len()).unwrap_or(i64::MAX);
            }
            let count = i64::try_from(messages.len()).unwrap_or(i64::MAX);
            conn.execute(
                "UPDATE agent_sessions SET message_count = message_count + ?2, \
                 tool_call_count = tool_call_count + ?3 WHERE id = ?1",
                params![session.0, count, tool_calls],
            )?;
            Ok(written)
        })
    }

    /// 세션의 메시지 전체. 번호 순서(쓴 순서)다. 세션이 없으면 [`StoreError::SessionNotFound`].
    pub fn session_messages(&self, session: SessionId) -> Result<Vec<AgentMessage>, StoreError> {
        self.read(|conn| {
            load_session(conn, session)?;
            let mut stmt = conn.prepare(&format!(
                "{MESSAGE_SELECT} WHERE session_id = ?1 ORDER BY id"
            ))?;
            let rows = stmt.query_map([session.0], message_from)?;
            rows.map(|row| row?).collect()
        })
    }
}

const SESSION_SELECT: &str = "SELECT id, agent, title, started_at, ended_at, end_reason, \
     message_count, tool_call_count FROM agent_sessions";

const MESSAGE_SELECT: &str = "SELECT id, session_id, role, content, api_content, tool_calls, \
     tool_call_id, tool_name, finish_reason, reasoning, at FROM agent_messages";

fn count(row: &Row<'_>, idx: usize) -> Result<u64, StoreError> {
    let raw: i64 = row.get(idx)?;
    u64::try_from(raw).map_err(|_| StoreError::Incompatible(format!("세기 값이 이상하다: {raw}")))
}

fn session_from(row: &Row<'_>) -> rusqlite::Result<Result<AgentSession, StoreError>> {
    let build = || -> Result<AgentSession, StoreError> {
        Ok(AgentSession {
            id: SessionId(row.get(0)?),
            agent: row.get(1)?,
            title: row.get(2)?,
            started_at: UnixMillis(row.get(3)?),
            ended_at: row.get::<_, Option<i64>>(4)?.map(UnixMillis),
            end_reason: row.get(5)?,
            message_count: count(row, 6)?,
            tool_call_count: count(row, 7)?,
        })
    };
    Ok(build())
}

fn load_session(conn: &Connection, id: SessionId) -> Result<AgentSession, StoreError> {
    conn.query_row(
        &format!("{SESSION_SELECT} WHERE id = ?1"),
        [id.0],
        session_from,
    )
    .optional()?
    .ok_or(StoreError::SessionNotFound(id))?
}

fn is_valid(message: &NewMessage) -> bool {
    let tool_needs_id = message.role != MessageRole::Tool || message.tool_call_id.is_some();
    let calls_only_assistant =
        message.role == MessageRole::Assistant || message.tool_calls.is_empty();
    tool_needs_id && calls_only_assistant
}

fn role_code(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

fn role_from(code: &str) -> Result<MessageRole, StoreError> {
    match code {
        "system" => Ok(MessageRole::System),
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        "tool" => Ok(MessageRole::Tool),
        other => Err(StoreError::Incompatible(format!(
            "모르는 메시지 role 값: {other}"
        ))),
    }
}

/// 도구 호출들을 JSON 글자로 적는다. 없으면 칸을 비운다.
fn tool_calls_text(calls: &[ToolCall]) -> Option<String> {
    if calls.is_empty() {
        return None;
    }
    let list: Vec<Value> = calls
        .iter()
        .map(|c| json!({ "id": c.id, "name": c.name, "arguments": c.arguments }))
        .collect();
    Some(Value::Array(list).to_string())
}

fn tool_calls_from(text: Option<String>) -> Result<Vec<ToolCall>, StoreError> {
    let Some(text) = text else {
        return Ok(Vec::new());
    };
    let bad = || StoreError::Incompatible("도구 호출 칸이 이상하다".to_string());
    let list: Vec<Value> = serde_json::from_str(&text)?;
    list.iter()
        .map(|item| {
            let field = |name: &str| item.get(name).and_then(Value::as_str).map(str::to_string);
            Ok(ToolCall {
                id: field("id").ok_or_else(bad)?,
                name: field("name").ok_or_else(bad)?,
                arguments: field("arguments").ok_or_else(bad)?,
            })
        })
        .collect()
}

fn message_from(row: &Row<'_>) -> rusqlite::Result<Result<AgentMessage, StoreError>> {
    let build = || -> Result<AgentMessage, StoreError> {
        Ok(AgentMessage {
            id: MessageId(row.get(0)?),
            session: SessionId(row.get(1)?),
            role: role_from(&row.get::<_, String>(2)?)?,
            content: row.get(3)?,
            api_content: row.get(4)?,
            tool_calls: tool_calls_from(row.get(5)?)?,
            tool_call_id: row.get(6)?,
            tool_name: row.get(7)?,
            finish_reason: row.get(8)?,
            reasoning: row.get(9)?,
            at: UnixMillis(row.get(10)?),
        })
    };
    Ok(build())
}

fn insert_message(
    conn: &Connection,
    session: SessionId,
    m: &NewMessage,
) -> Result<AgentMessage, StoreError> {
    conn.execute(
        "INSERT INTO agent_messages (session_id, role, content, api_content, tool_calls, \
         tool_call_id, tool_name, finish_reason, reasoning, at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            session.0,
            role_code(m.role),
            m.content,
            m.api_content,
            tool_calls_text(&m.tool_calls),
            m.tool_call_id,
            m.tool_name,
            m.finish_reason,
            m.reasoning,
            m.at.0,
        ],
    )?;
    Ok(AgentMessage {
        id: MessageId(conn.last_insert_rowid()),
        session,
        role: m.role,
        content: m.content.clone(),
        api_content: m.api_content.clone(),
        tool_calls: m.tool_calls.clone(),
        tool_call_id: m.tool_call_id.clone(),
        tool_name: m.tool_name.clone(),
        finish_reason: m.finish_reason.clone(),
        reasoning: m.reasoning.clone(),
        at: m.at,
    })
}
