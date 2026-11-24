//! pi-office: 사무실 서버(구성 요소 A3). 시각은 Clock으로, 이벤트 보내기는 포트로 받는다. 화면은 server(`crates/server/web`)에 있다.
//!
//! 작업 지시서: `docs/dev/modules/office.md`.

pub mod chat;
pub mod error;
pub mod mention;
pub mod notice;
pub mod office;
pub mod place;
pub mod port;
pub mod summon;

pub use summon::{STALE_WORK_MS, SUMMON_ARRIVE_MS, SUMMON_IDLE_MS, SummonPhase, SummonReason};

pub use chat::{ChatResponse, ChatStatus, Room};
pub use error::{Error, SinkError};
pub use mention::{
    Member, RELAY_BUDGET, Relayed, TeamHint, TeamPost, mention_tag, mentioned_names, parse_mentions,
};
pub use notice::{NoticeAckTurn, NoticePost, NoticeTurn};
pub use office::Office;
pub use place::{Activity, Agent, AgentView, Place, TeamState, Work, place_of};
pub use port::{AiBudget, Clock, EventSink, Redactor};

/// 공지를 받았을 때 내는 이벤트 종류(6차, C19). 내용: `{"notice", "text", "at"}`(글은 가린 것).
pub const EVENT_NOTICE: &str = "office.notice";

/// 담당별 공지 확인 상태가 바뀔 때 내는 이벤트 종류(6차, C19).
/// 내용: `{"notice", "agent", "state"}`, `state`는 `"answered"` · `"limit"` · `"failed"`.
pub const EVENT_NOTICE_ACK: &str = "office.notice_ack";

/// 담당 모습이 바뀔 때 내는 이벤트 종류. 내용: `{"agent", "place", "work", "phase", "reason", "phaseAt"}`
/// (앞의 세 칸은 옛 칸, 뒤의 세 칸은 5차에 더한 부르기 단계 칸).
pub const EVENT_AGENT: &str = "office.agent";

/// 응답 객체가 바뀔 때 내는 이벤트 종류. 내용: 응답 객체 전체([`ChatResponse`]).
pub const EVENT_CHAT: &str = "office.chat";

/// 담당마다 끝나지 않은 응답을 둘 수 있는 개수(원본 DeskRPG `SessionQueue(8)`).
pub const CHAT_QUEUE_MAX: usize = 8;
