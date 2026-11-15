//! 공용 도우미: 대본대로 답하는 가짜 AI, 가짜 도구, 고정 시계, 표시 가리기, 저장소.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use pi_agent::{
    ActionCall, Clock, DataItem, DiscussionLimits, DiscussionRequest, OrderDraft, Ports, Role,
    RoleConfig, Team, TimeLimits, ToolError, Tools, TurnRunner,
};
use pi_ai::{
    AiError, AiProvider, CompletionRequest, CompletionResponse, Redactor, StopReason, Usage,
};
use pi_store::{AccountKind, MessageRole, NewTrade, RoomEvent, Store, TradeId, UnixMillis};
use tempfile::TempDir;

/// 테스트의 지금 시각.
pub const NOW: UnixMillis = UnixMillis(1_763_000_000_000);

/// 가려져야 하는 표시 글자(진짜 키 모양이 아니다).
pub const SECRET_MARK: &str = "zz-not-a-real-key-0000";

/// 가린 자리.
pub const MASK: &str = "[가림]";

/// 시험 종목.
pub const SYMBOL: &str = "005930";

/// 담당의 모델 이름. 가짜 AI는 모델 이름으로 담당을 가른다.
pub fn model(role: Role) -> String {
    format!("model-{role:?}")
}

/// 가짜 AI의 대본 한 줄.
#[derive(Clone)]
pub enum Step {
    /// 이 글로 답한다.
    Say(String),
    /// 이만큼(초) 기다렸다가 답한다.
    Slow(u64, String),
    /// 이 오류를 낸다.
    Fail(AiError),
    /// 답하기 전에 이 일을 한다.
    Hook(Arc<dyn Fn() + Send + Sync>, String),
    /// 이 글로 끝없이 답한다(꺼내지 않는다).
    Forever(String),
}

/// 대본 줄 `Say`.
pub fn say(text: &str) -> Step {
    Step::Say(text.to_string())
}

/// 행동 JSON 하나가 든 답 글.
pub fn action(name: &str) -> String {
    format!("자료를 봅니다.\n{{\"action\": \"{name}\", \"args\": {{\"symbol\": \"{SYMBOL}\"}}}}")
}

/// 점수 턴의 답 글.
pub fn score_reply(score: i32, reason: &str) -> String {
    format!("```json\n{{\"score\": {score}, \"reason\": \"{reason}\"}}\n```")
}

/// 팀장의 사기 판단 답 글(근거 칸 모두 채움).
pub fn leader_buy() -> String {
    "{\"decision\": \"buy\", \"hypothesis\": \"실적 개선\", \"trigger\": \"공시\", \
     \"entry\": 70000, \"stop\": 65000, \"target\": 80000, \"expected_value\": 3000}"
        .to_string()
}

/// 매매 담당의 사기 주문 안 답 글.
pub fn trader_buy(note: &str) -> String {
    format!("{{\"side\": \"buy\", \"note\": \"{note}\"}}")
}

/// 대본대로 답하는 가짜 AI. 받은 요청을 모두 남긴다.
#[derive(Default)]
pub struct FakeAi {
    scripts: Mutex<HashMap<String, VecDeque<Step>>>,
    requests: Mutex<Vec<CompletionRequest>>,
}

impl FakeAi {
    /// 담당의 대본을 뒤에 더한다.
    pub fn script(&self, role: Role, steps: Vec<Step>) {
        let mut scripts = self.scripts.lock().unwrap();
        scripts.entry(model(role)).or_default().extend(steps);
    }

    /// 받은 요청 전부.
    pub fn requests(&self) -> Vec<CompletionRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// 그 담당이 받은 요청 수.
    pub fn calls(&self, role: Role) -> usize {
        let name = model(role);
        self.requests().iter().filter(|r| r.model == name).count()
    }

    fn next(&self, model: &str) -> Option<Step> {
        let mut scripts = self.scripts.lock().unwrap();
        let queue = scripts.get_mut(model)?;
        match queue.front() {
            Some(Step::Forever(_)) => queue.front().cloned(),
            _ => queue.pop_front(),
        }
    }
}

#[async_trait]
impl AiProvider for FakeAi {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, AiError> {
        let step = self.next(&request.model);
        self.requests.lock().unwrap().push(request);
        let text = match step {
            None => return Err(AiError::InvalidResponse),
            Some(Step::Fail(error)) => return Err(error),
            Some(Step::Say(text)) | Some(Step::Forever(text)) => text,
            Some(Step::Slow(secs, text)) => {
                tokio::time::sleep(Duration::from_secs(secs)).await;
                text
            }
            Some(Step::Hook(hook, text)) => {
                hook();
                text
            }
        };
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

/// 가짜 도구. 아는 이름은 `quote` · `filing` · `boom`(언제나 실패)이다.
pub struct FakeTools {
    store: Store,
    calls: Mutex<Vec<(Role, ActionCall)>>,
    persisted_first: Mutex<Vec<bool>>,
    drafts: Mutex<Vec<OrderDraft>>,
}

impl FakeTools {
    /// 부른 도구들.
    pub fn calls(&self) -> Vec<(Role, ActionCall)> {
        self.calls.lock().unwrap().clone()
    }

    /// 도구를 부를 때마다 "이미 저장돼 있었는가".
    pub fn persisted_first(&self) -> Vec<bool> {
        self.persisted_first.lock().unwrap().clone()
    }

    /// 넘겨받은 주문 안들.
    pub fn drafts(&self) -> Vec<OrderDraft> {
        self.drafts.lock().unwrap().clone()
    }

    fn stored_calls(&self, agent: &str, name: &str) -> usize {
        let sessions = self.store.sessions_of(agent).unwrap();
        let Some(open) = sessions.iter().rev().find(|s| s.ended_at.is_none()) else {
            return 0;
        };
        let messages = self.store.session_messages(open.id).unwrap();
        messages
            .iter()
            .filter(|m| m.role == MessageRole::Assistant)
            .flat_map(|m| m.tool_calls.iter())
            .filter(|c| c.name == name)
            .count()
    }
}

#[async_trait]
impl Tools for FakeTools {
    fn names(&self, _role: Role) -> Vec<String> {
        vec!["quote".into(), "filing".into(), "boom".into()]
    }

    async fn call(&self, role: Role, call: &ActionCall) -> Result<String, ToolError> {
        let before = self
            .calls()
            .iter()
            .filter(|(r, c)| *r == role && c.name == call.name)
            .count();
        let stored = self.stored_calls(role.id(), &call.name);
        self.persisted_first.lock().unwrap().push(stored > before);
        self.calls.lock().unwrap().push((role, call.clone()));
        if call.name == "boom" {
            return Err(ToolError("도구가 터졌다".into()));
        }
        Ok(format!("결과:{}", call.name))
    }

    async fn submit_order_draft(&self, draft: &OrderDraft) -> Result<(), ToolError> {
        self.drafts.lock().unwrap().push(draft.clone());
        Ok(())
    }
}

/// 표시 글자를 가린다.
pub struct MarkRedactor;

impl Redactor for MarkRedactor {
    fn redact(&self, text: &str) -> String {
        text.replace(SECRET_MARK, MASK)
    }
}

/// 고정 시계.
pub struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> UnixMillis {
        NOW
    }
}

/// 한 테스트의 환경: 임시 저장소 · 가짜 AI · 가짜 도구.
pub struct Env {
    _dir: TempDir,
    pub store: Store,
    pub ai: Arc<FakeAi>,
    pub tools: Arc<FakeTools>,
}

impl Env {
    pub fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("agent.db")).unwrap();
        let tools = Arc::new(FakeTools {
            store: store.clone(),
            calls: Mutex::default(),
            persisted_first: Mutex::default(),
            drafts: Mutex::default(),
        });
        Env {
            _dir: dir,
            store,
            ai: Arc::new(FakeAi::default()),
            tools,
        }
    }

    pub fn ports(&self) -> Ports {
        Ports {
            ai: self.ai.clone(),
            tools: self.tools.clone(),
            store: self.store.clone(),
            redactor: Arc::new(MarkRedactor),
            clock: Arc::new(FixedClock),
        }
    }

    pub fn runner(&self) -> TurnRunner {
        TurnRunner::new(self.ports())
    }

    pub fn team(&self) -> Team {
        let roles = Role::ALL
            .iter()
            .map(|role| RoleConfig::new(*role, model(*role)))
            .collect();
        Team::new(self.ports(), roles, TimeLimits::MARKET).unwrap()
    }

    /// 논의중인 새 매매.
    pub fn trade(&self) -> TradeId {
        let new = NewTrade {
            symbol: SYMBOL.into(),
            account: AccountKind::Paper,
            at: UnixMillis(NOW.0 - 60_000),
        };
        self.store.create_trade(new).unwrap().id
    }

    /// 방 로그 전체.
    pub fn room(&self, room: &str) -> Vec<RoomEvent> {
        self.store.room_events(room, 0, 1000).unwrap()
    }

    /// 담당 id의 모든 세션 메시지 글을 이어 붙인 것.
    pub fn session_text(&self, agent: &str) -> String {
        let mut all = String::new();
        for session in self.store.sessions_of(agent).unwrap() {
            for message in self.store.session_messages(session.id).unwrap() {
                all.push_str(message.content.as_deref().unwrap_or(""));
                all.push('\n');
            }
        }
        all
    }
}

/// 자료 하나(가져온 것).
pub fn data(name: &str) -> DataItem {
    DataItem {
        name: name.into(),
        content: Some(format!("{name} 내용")),
        known_at: UnixMillis(NOW.0 - 1_000),
    }
}

/// 방 `room-1`에서 뉴스 공시 시세 · 전략 · 리스크 담당이 논의하는 요청. 0라운드는 뉴스 공시 시세 담당.
pub fn request(trade: TradeId) -> DiscussionRequest {
    DiscussionRequest {
        room: "room-1".into(),
        trade,
        topic: "삼성전자 실적 공시".into(),
        members: vec![Role::Scout, Role::Strategy, Role::Risk],
        nominated: vec![Role::Scout],
        data: vec![data("공시"), data("시세")],
        consensus_required: 150,
        limits: DiscussionLimits::default(),
    }
}

/// 점수 턴 · 팀장 · 매매 담당 대본: 점수 70 · 80 · 60(합 210), 사기, 주문 안.
pub fn script_after_discussion(ai: &FakeAi) {
    ai.script(Role::Scout, vec![Step::Say(score_reply(70, "공시가 좋다"))]);
    ai.script(
        Role::Strategy,
        vec![Step::Say(score_reply(80, "추세 상승"))],
    );
    ai.script(Role::Risk, vec![Step::Say(score_reply(60, "비중 작게"))]);
    ai.script(Role::Leader, vec![Step::Say(leader_buy())]);
    ai.script(Role::Trader, vec![Step::Say(trader_buy("공시 뒤 사기"))]);
}

/// 그 종류의 방 이벤트들.
pub fn of_kind<'a>(events: &'a [RoomEvent], kind: &str) -> Vec<&'a RoomEvent> {
    events.iter().filter(|e| e.kind == kind).collect()
}

/// 방 로그의 지켜보기 까닭 코드들.
pub fn watch_codes(events: &[RoomEvent]) -> Vec<String> {
    of_kind(events, "room.activity")
        .iter()
        .filter(|e| e.payload["status"] == "watch")
        .map(|e| e.payload["reason_code"].as_str().unwrap_or("").to_string())
        .collect()
}
