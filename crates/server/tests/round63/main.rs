//! server 백로그 Q 시험(돈 · 안전과 승인 · 알림 정리).
//!
//! - `scout`: 뉴스 공시 시세 담당의 깨우기 턴이 쓴 세션(`wake-…`)은 턴이 끝나면 닫힌다(쌓이지 않는다).
//! - `card`: 사무실 승인 카드(`office.approval` → `GET /api/approvals/{번호}` → `POST …/approve`)로 규칙 개선안을
//!   승인하면 개선안 답하기와 같게 반영된다(35차에서 더해진 개선안 승인 이벤트의 길). 개선안 목록은 뒤에 생긴
//!   개선안의 승인 번호도 싣는다(개선안 이벤트를 매번 처음부터 읽지 않게 바꿔도 같다).
//! - `live`: 실전중 템플릿의 사는 주문이 실행기에서 막히면(E25) 그 매매에 막힌 까닭이 남는다.
//!
//! 공용 도우미는 round22(깨우기) · round30(승인 카드) · round17(실전중 템플릿)의 것을 그대로 쓴다(그 폴더들은
//! 고치지 않는다). 바깥 서비스는 모두 로컬 가짜 서버이고 계좌는 모의계좌뿐이다.

#[allow(dead_code)]
#[path = "../round30/common.rs"]
mod common;

#[allow(dead_code)]
#[path = "../round22/common.rs"]
mod waking;

#[allow(dead_code)]
#[path = "../round17/common.rs"]
mod promoting;

mod card;
mod live;
mod scout;
