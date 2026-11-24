//! 24차 시연 모드에서 팀이 실제로 돈다(`docs/dev/modules/server.md` "24차 시연 모드에서 팀이 실제로 돈다",
//! additions.md R2-3 · D14 · C33 · E56 · N19 · V54)의 테스트.
//!
//! 잠긴 테스트 파일을 고치지 않으려고 따로 둔 최상위 테스트 바이너리다. 서버는 실행 파일과 같은 조립
//! ([`price_inha::run_assembly`] → [`price_inha::App::launch_run`])을 쓰되 시계 · 바깥 주소 · 가짜 공시 간격만
//! 시험이 정한다. "실제" 증권사 · AI · DART · 네이버 주소는 모두 로컬 가짜 서버(받은 요청을 센다)다.
//!
//! - `calendar`: 1번 시연 달력의 모양(매일 거래일 · 시장 시간대 00:05~23:55 · 자정 앞뒤 10분은 장 밖).
//! - `assembly`: 1번 · 3번 실행 파일 조립 값(`--demo`일 때만 시연 달력 · 3분 가짜 공시).
//! - `day`: 2번 시연 조립으로 주말 · 밤에도 첫 tick이 성공하고 장 시간이면 하루를 시작한다 · 자정 앞뒤는 장 밖.
//! - `disclosure`: 2 · 3번 가짜 공시 간격을 줄이면 종목 논의가 열리고 담당 글 `office.talk`가 나온다.
//! - `outbound`: 4번 시연은 바깥으로 요청 0건(키가 있어도) · 실제 조립에는 시연 달력이 섞이지 않는다.

mod assembly;
mod calendar;
mod common;
mod day;
mod disclosure;
mod outbound;
