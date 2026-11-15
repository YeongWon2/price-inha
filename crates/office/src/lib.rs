//! pi-office: 사무실 서버(구성 요소 A3)와 3D 사무실 화면(A1)의 자산. 시각은 Clock으로, 이벤트 보내기는 포트로 받는다.
//!
//! 작업 지시서: `docs/dev/modules/office.md`.

pub mod chat;
pub mod error;
pub mod office;
pub mod place;
pub mod port;
pub mod web;

pub use chat::{ChatResponse, ChatStatus, Room};
pub use error::{Error, SinkError};
pub use office::Office;
pub use place::{Activity, Agent, AgentView, Place, TeamState, Work, place_of};
pub use port::{Clock, EventSink};
pub use web::{INDEX_PATH, MAP_PATH, WebAsset, web_assets};

/// 담당 모습이 바뀔 때 내는 이벤트 종류. 내용: `{"agent", "place", "work"}`.
pub const EVENT_AGENT: &str = "office.agent";

/// 응답 객체가 바뀔 때 내는 이벤트 종류. 내용: 응답 객체 전체([`ChatResponse`]).
pub const EVENT_CHAT: &str = "office.chat";

/// 담당마다 끝나지 않은 응답을 둘 수 있는 개수(원본 DeskRPG `SessionQueue(8)`).
pub const CHAT_QUEUE_MAX: usize = 8;
