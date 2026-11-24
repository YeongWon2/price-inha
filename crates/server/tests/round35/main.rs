//! server 33차 시험: 멘션 · 전체 공지 · 담당 부르기를 API에 잇기(C18 · C19 · C20 · V58~V60 · V71~V74).
//!
//! - `mention`: 멘션된 담당만 답함 · 여럿이면 차례 · 멘션 없으면 `no_mention` · 없는 이름 `no_match` ·
//!   `agent`를 주면 지금처럼 · 서버 명단 = 화면 지도 이름 · 멘션 답 시작에 `office.summon` called(mention) · 방 바꾸면 returning.
//! - `notice`: 저장 · 여섯 확인 · `office.notice` · `office.notice_ack` · 지난 공지 읽기 · 같은 번호 한 번 · 상한이면 limit ·
//!   가림 · 한도 · 공지로 아무것도 바뀌지 않음 · 다음 논의 자료에 들어감(판단 뒤에 보낸 것은 안 들어감).
//! - `summon`: called → arrived → waiting → 10초 뒤 returning → 도착 알림에 idle · 대화가 있으면 10초 다시 셈 ·
//!   30초 도착 없음 → idle · 부름으로 온 담당은 방을 바꿔도 남음 · 모르는 담당 400 · 회의 중 409.
//! - `gate`: 세션 없음(원격 포함) 401 · 다른 출처 거부 · 바깥 가짜 서버로 간 요청 0건.
//!
//! 시간(10초 · 30초 · 보내기 간격 2초)은 넣어 준 시계를 옮겨 잰다(실제로 자지 않는다).

mod common;
mod gate;
mod mention;
mod notice;
mod summon;
