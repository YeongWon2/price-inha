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

use pi_ai::{AiProvider, Redactor};
use pi_store::{SessionId, Store};

use crate::error::Error;
#[cfg(doc)]
use crate::error::TurnFailure;
use crate::ports::{Clock, Tools};
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
        let _ = (&self.ports, config, limits, session_title, input);
        todo!()
    }
}
