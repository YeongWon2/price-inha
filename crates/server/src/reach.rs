//! (W5 3차) 검색 · 백업 · 사무실 · 폰 잇기: T16 · T19 · T20 · T21 · T22 연결.
//!
//! 작업 지시서: `docs/dev/modules/server.md` "3차"와 "사무실 화면을 조립할 때".
//!
//! [`App::launch_with`]가 2차까지의 조립([`App::launch`])에 더해 아래를 잇는다. [`App::launch`]는 2차까지와
//! 같게 동작하고(이벤트 · 알림을 더 내지 않는다), 웹 자산 경로만 함께 단다.
//!
//! - 웹 자산(공개 경로, 세션 없이 열린다 · Host와 상대 주소 확인은 받는다):
//!   화면 빌드 결과(`web/dist`, server 11차)의 사무실 쪽은 [`OFFICE_ASSETS_PREFIX`] 아래(`GET /office/`는
//!   `office/index.html`), 폰 쪽은 [`PHONE_ASSETS_PREFIX`] 아래(`GET /phone/`는 `phone/index.html`), 두 쪽이
//!   부르는 묶음은 `/assets/` 아래에 단다. `Content-Type`은 파일 끝으로 정하고, 없는 파일은 404다.
//! - 사무실(`pi_office::Office`): 이벤트 포트는 접속 관문의 `Publisher::publish`(번호 있는 이벤트, E37),
//!   시계는 [`crate::Wiring::clock`]. 팀 상태가 바뀌면 `set_team_state`, 논의에 든 담당은
//!   `Activity::Discussing`(끝나면 `Working`)으로 알린다(V13 · V15). 경로 [`OFFICE_PATH`] · [`OFFICE_CHAT_PATH`].
//! - 알림(`pi_notify::Notifier`): VAPID 키는 비밀 보관소 폴더의 `pi_notify::VAPID_KEY_FILE`(0600), 가리기는
//!   비밀 보관소, 시계는 [`crate::Wiring::clock`]. 알림을 내는 일:
//!   승인 요청(주문이 승인을 기다리면, 주소 `/phone/#/approvals/<승인 번호>`, V10),
//!   이상(지켜보는 종목의 시세 늦음이 없음 → 있음으로 바뀔 때, V11 · N5),
//!   접속 잠금(`pi_gate::LockAlert`, E2 · V39), 백업 실패(E15), 하루 정리(하루 마감 뒤, V12).
//!   경로 [`PUSH_KEY_PATH`] · [`PUSH_SUBSCRIBE_PATH`] · [`NOTICES_PATH`] · [`APPROVAL_PATH`] ·
//!   [`APPROVAL_DECIDE_PATH`] · [`DAILY_SUMMARY_PATH`].
//! - 지식 검색(`pi_knowledge::Knowledge`, 임베딩 없음): 복기가 남으면 그 매매의 기록(종류 `trade`)과 복기
//!   (종류 `review`)를 **가린 글로** 색인한다(기록 키는 매매 번호 글자). 논의를 시작할 때 종목 · 계기로 자동
//!   회상해 찾은 지난 매매를 논의 자료로 넣는다: 판단 기록의 자료 이름은 [`RECALL_SOURCE_PREFIX`]`<매매 번호>`
//!   (V22). 담당 도구 [`KNOWLEDGE_TOOL`]은 [`crate::Crew::search_knowledge`]와 같은 길이다. 바깥 요청은 없다(V23).
//! - 백업(`pi_backup::Backup`, `reach.backup`이 있을 때): 하루 마감([`crate::KIND_DAY_CLOSE`])을 남긴 뒤 한 번
//!   올린다(V48). 실패하면 백업 실패 알림을 내고 다음 마감에 다시 한다(E15). 토큰은 비밀 보관소의
//!   `KeySlot::BackupGithubToken`이다. 저장소 · 공개키는 설정 화면([`SETUP_BACKUP_PATH`])이 바꾼다. 되살리기는
//!   [`restore_backup`](`pi_backup::restore_in`, 작업 폴더는 그 옆)으로 빈 폴더에 하고, 되살린 폴더로 띄우면
//!   팀은 전체 정지 · 승인은 만료이며 키를 다시 넣고 [`crate::TEAM_RESTART_PATH`]를 누르기 전에는 팀이 돌지
//!   않는다(V51 · V52, C8 · C16).
//!
//! 새 API 경로는 모두 세션 · Origin 확인을 받는다(`protected_routes`).

use std::sync::Arc;
use std::time::Duration;

mod assets;
mod backup_http;
mod linked;
mod looks_http;
mod office_http;
mod phone_http;
mod semantic;
mod semantic_http;
mod sentence;
mod summary;
mod talk_feed;

pub(crate) use assets::routes as asset_routes;
pub(crate) use backup_http::GITHUB_API_BASE;
pub use backup_http::{BACKUP_SETTINGS_FILE, SETUP_BACKUP_PATH, restore_backup};
pub(crate) use backup_http::{clear_restore_leftover, routes as backup_routes};
pub(crate) use linked::{Linked, LockNotices};
pub(crate) use looks_http::{Looks, routes as looks_routes};
pub(crate) use office_http::{OfficeDesk, routes as office_routes};
pub(crate) use phone_http::{Phone, routes as phone_routes};
pub(crate) use semantic::{Semantic, SemanticSettings};
pub(crate) use semantic_http::routes as semantic_routes;
pub(crate) use summary::daily_summary;
pub(crate) use talk_feed::{Following, TalkFeed, routes as talk_routes};

/// 사무실 화면 자산을 다는 자리. `GET /office/`는 `index.html`, `GET /office/<경로>`는 그 파일.
pub const OFFICE_ASSETS_PREFIX: &str = "/office/";

/// 폰 화면 자산을 다는 자리(묶음 manifest의 `id`와 같다). `GET /phone/`는 `index.html`.
pub const PHONE_ASSETS_PREFIX: &str = "/phone/";

/// 사무실 접속 스냅샷. `GET` → `{"agents": [{"agent", "place", "work"}], "pending": [응답 객체],
/// "last_seq": 수}`. 마지막 번호를 **먼저** 읽고 모습 · 끝나지 않은 응답을 읽는다(그 뒤의 이벤트는
/// `/ws?last_seq=`로 받는다, E3). 담당 id는 `pi_office::Agent::id`(`news` · `strategy` · `risk` · `review` ·
/// `trader` · `leader`)다. (22차 1번) `"talk"` 칸에 최근 팀 대화([`OFFICE_TALK_PATH`]와 같은 줄, `last_seq`까지)를
/// 이벤트 번호(`"seq"`)를 붙여 시각 순서로 준다. 화면은 이것으로 첫 화면을 그리고 그 뒤는 [`KIND_OFFICE_TALK`]로 받는다.
pub const OFFICE_PATH: &str = "/api/office";

/// 담당에게 말하기(C5). `POST` `{"room": "team" | "direct", "agent": 담당 id, "text": 글,
/// "sourceMessageId": 글 번호}` → 200 응답 객체(queued, `office.chat`과 같은 모양).
///
/// - 글이 비었거나 모르는 방 · 담당이면 400 `{"error": `[`OFFICE_BAD_INPUT`]`}`.
/// - 글이 [`OFFICE_TEXT_MAX`]자를 넘거나 `sourceMessageId`가 비었거나 [`OFFICE_SOURCE_ID_MAX`]자를 넘으면
///   400 `{"error": `[`OFFICE_TOO_LONG`]`}`(빈 번호는 [`OFFICE_BAD_INPUT`]).
/// - 같은 기기(상대 IP)가 [`OFFICE_SEND_INTERVAL`] 안에 또 보내면 429 `{"error": `[`OFFICE_TOO_FAST`]`}`.
///   간격은 [`crate::Wiring::clock`]으로 재고, 거부한 글은 간격에 세지 않는다.
/// - 담당의 대기열이 차면 429 `{"error": "queue_full"}`.
/// - 팀 방의 글은 [`KIND_OFFICE_MESSAGE`] 이벤트로도 낸다.
/// - 답은 뒤에서 그 담당의 AI 턴으로 만든다(시연 모드는 `DemoTurn::Chat`). thinking → complete,
///   답하지 못하면 failed(E6). 무응답 [`CHAT_IDLE_TIMEOUT`] · 전체 [`CHAT_TOTAL_TIMEOUT`]이면 failed다.
pub const OFFICE_CHAT_PATH: &str = "/api/office/chat";

/// 담당별 외형(20차 4번). `GET` → 200 `{"agents": {담당 id: 외형 id(여섯 모두)}, "available": [원본 외형 id 50개]}`.
/// 외형 id는 원본 `characters/office/<id>.glb`의 `<id>`다. 처음 값(기본 외형)은 여섯 모두 서로 다르다.
/// 세션이 있어야 열린다(없으면 401).
pub const OFFICE_LOOKS_PATH: &str = "/api/office/looks";

/// 담당의 외형 바꾸기(20차 4번). `POST` `{"agent": 담당 id, "look": 외형 id}` → 200 [`OFFICE_LOOKS_PATH`]의 `GET`과
/// 같은 모양. 모르는 담당 · 목록에 없는 외형이면 400(저장하지 않는다). 세션이 없으면 401, 같은 출처가 아니면 4xx.
/// 저장은 서버 쪽이라 서버를 다시 켜도, 다른 기기에서도 같다.
pub const OFFICE_LOOK_PATH: &str = "/api/office/look";

/// (22차 1번) 사무실 팀 대화의 최근 줄. `GET` → 200 `[{"room", "agent", "text", "at", "topic"}, …]`: 저장된
/// [`KIND_OFFICE_TALK`] 이벤트 가운데 최근 [`OFFICE_TALK_RECENT`]줄을 시각 순서(오래된 것 먼저)로 준다. 이벤트 내용
/// 그대로다. 세션이 있어야 열린다(없으면 401).
pub const OFFICE_TALK_PATH: &str = "/api/office/talk";

/// [`OFFICE_TALK_PATH`]가 주는 최근 줄 수.
pub const OFFICE_TALK_RECENT: usize = 50;

/// (22차 1번) 저장되는 이벤트 종류(번호 있는 이벤트): 논의 방(`trade-<번호>` · [`crate::DAY_ROOM`])에 담당 글
/// (`message.member`, 팀장 판단 글 포함)이 쌓이면 1초 안에 하나씩 낸다. 내용 `{"room": 방 이름, "agent": 방 로그
/// 글의 쓴 쪽 id, "text": 방 로그 글(가린 것, [`OFFICE_TEXT_MAX`]자에서 자름), "at": 방 로그 글의 시각(유닉스
/// 밀리초), "topic": 종목 코드 | "day"}`. 점수 턴 · 실패 줄(`turn.failed`) · 진행 줄(`room.activity`)은 내지 않는다.
pub const KIND_OFFICE_TALK: &str = "office.talk";

/// 사무실 글 길이 상한(원본 DeskRPG `npc:chat` 500자).
pub const OFFICE_TEXT_MAX: usize = 500;

/// (25차 2번) 팀장(`leader`)의 판단 글(방 로그 원문)을 [`KIND_OFFICE_TALK`]의 `text`로 낼 사람이 읽는 문장.
/// 방 로그의 원문은 바꾸지 않는다. 문장도 가린 뒤 [`OFFICE_TEXT_MAX`]자에서 자른다.
///
/// - 하루 판단(`plan` 칸이 있는 JSON): `trade` → "오늘은 운용합니다.", `rest` → "오늘은 쉽니다 — {이유}".
/// - 종목 판단(`decision` 칸이 있는 JSON): `buy` → "사기로 정했습니다 — 진입 {진입} · 손절 {손절} · 목표 {목표}.
///   {가설}", `sell` → "팔기로 정했습니다 — …"(같은 모양), `watch` → "지켜보기로 했습니다 — {이유}".
///   금액은 저장소 금액(통화의 최소 단위)을 그 시장(`market`)의 돈 모양으로 쓴다: 국내는 원 단위에 세 자리마다
///   쉼표(`71,100`), 미국은 센트 ÷ 100의 소수 둘째 자리(`187.20`).
/// - JSON으로 읽히지 않는 글(판단 모양이 아닌 글)은 `None` — 부르는 쪽이 원문을 그대로 쓴다.
pub fn leader_sentence(text: &str, market: pi_market::Market) -> Option<String> {
    sentence::sentence(text, market)
}
/// `sourceMessageId` 길이 상한. 원본은 UUID(36자)를 쓴다. 넉넉히 128자로 정했다.
pub const OFFICE_SOURCE_ID_MAX: usize = 128;
/// 같은 기기가 글을 보내는 가장 짧은 간격(원본 2초).
pub const OFFICE_SEND_INTERVAL: Duration = Duration::from_secs(2);
/// 채팅 답의 무응답 시간 제한.
pub const CHAT_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
/// 채팅 답의 전체 시간 제한.
pub const CHAT_TOTAL_TIMEOUT: Duration = Duration::from_secs(600);
/// 저장되는 이벤트 종류: 팀 방의 사용자 글. 내용 `{"room": "team", "agent": 담당 id, "text": 가린 글,
/// "sourceMessageId": 글 번호}`.
pub const KIND_OFFICE_MESSAGE: &str = "office.message";
/// 사무실 거부: 입력이 잘못됐다.
pub const OFFICE_BAD_INPUT: &str = "invalid_input";
/// 사무실 거부: 너무 길다.
pub const OFFICE_TOO_LONG: &str = "too_long";
/// 사무실 거부: 너무 자주 보낸다.
pub const OFFICE_TOO_FAST: &str = "too_fast";

/// VAPID 공개키. `GET` → `{"key": base64url}`.
pub const PUSH_KEY_PATH: &str = "/api/push/key";

/// 웹 푸시 구독. `POST` `{"endpoint", "p256dh", "auth", "device"}` → 204.
///
/// - 키 모양이 틀렸거나 주소가 `https://`가 아니면 400 `{"error": "invalid_input"}`.
/// - 주소의 이름이 내부망이면 400 `{"error": `[`SUBSCRIBE_REFUSED`]`}`(SSRF 막기, 해석 전 글자로 본다):
///   `localhost` · `*.localhost` · `*.local` · `*.internal`, 루프백 · 사설 · 링크 로컬 · 지정되지 않은 IP,
///   IPv6 고유 로컬(`fc00::/7`) · 링크 로컬 · 루프백.
pub const PUSH_SUBSCRIBE_PATH: &str = "/api/push/subscribe";
/// 구독 거부: 내부망 주소.
pub const SUBSCRIBE_REFUSED: &str = "endpoint_not_allowed";

/// 알림 목록. `GET ?after=<번호>&max=<개수>` → `[{"seq", "kind", "title", "body", "link"?, "at"}]`
/// (번호 순서, `max` 기본 50 · 최대 200). 가린 글이다.
pub const NOTICES_PATH: &str = "/api/notices";

/// 승인 카드 읽기. `GET /api/approvals/{id}` → `{"number", "summary", "fingerprint"(64자), "short_code"(6자),
/// "status"}`. 없는 번호는 404 `{"error": "not_found"}`.
pub const APPROVAL_PATH: &str = "/api/approvals/{id}";

/// 폰의 승인 · 거절. `POST /api/approvals/{id}/approve | reject` `{"fingerprint", "code"?}`.
/// `code`(6자리)는 읽지 않는다(결정 4번: 사용자가 치지 않는다, 지문으로 견준다). 처리는
/// [`crate::APPROVAL_ANSWER_PATH`]와 같고 몸통 `{"result", "reason"}`도 같다. 승인을 눌렀는데 결과가 거절이면
/// 409, 그 밖은 200이다. 다른 동사는 404.
pub const APPROVAL_DECIDE_PATH: &str = "/api/approvals/{id}/{verb}";

/// 하루 정리(V12 · V50). `GET` → `{"date": "YYYY-MM-DD", "done": 글, "pnl": 글, "review": 글,
/// "orders": 그날 체결된 주문 수, "pnl_krw": 그날 시작 평가액에서 지금까지의 손익(십진수 글자),
/// "reviews": 그날 남긴 복기 수, "backup_at": 마지막 백업 성공 시각(유닉스 밀리초) | null}`.
/// 날짜는 시장 달력의 오늘이다. 백업이 있으면 `done`에 "백업 완료"와 그 시각이 들어간다.
///
/// (29차 1번 · C30 · V84) `GET ?date=YYYY-MM-DD`로 그 시장 날짜의 하루 정리를 읽는다.
///
/// - 날짜가 없거나 시장 달력의 오늘과 같으면 지금과 같은 응답이다(한 글자도 다르지 않다).
/// - 다른 날짜면 같은 모양이고 `date`는 준 날짜, `orders` · `reviews` · `done` · `review`는 그 날짜의 것이다
///   (`orders`: 그 날짜에 낸 주문 가운데 다 체결된 수, `reviews`: 그 날짜에 남긴 복기 수).
///   그날 시작 평가액은 남아 있지 않으므로 `pnl_krw`는 `null`이고 `pnl`은 그날 손익 기록이 없다는 글이다.
///   `backup_at`은 날짜와 상관없이 마지막 백업 성공 시각이다.
/// - 날짜 글자가 `YYYY-MM-DD` 모양의 있는 날짜가 아니면 400 `{"error": "invalid_input"}`.
/// - 세션이 있어야 열린다(원격은 세션 없이 401).
pub const DAILY_SUMMARY_PATH: &str = "/api/daily-summary";

/// (29차 1번 · C30 · V83) 히스토리 화면의 매매 목록. `GET ?before=&max=&state=` → 200
///
/// ```text
/// {
///   "trades": [{"id": 매매 번호, "symbol", "account": "paper" | "live",
///               "state": 매매 상태 글자, "created_at": 유닉스 밀리초, "updated_at": 유닉스 밀리초,
///               "decided_at": 유닉스 밀리초 | null}],
///   "next": 다음 쪽의 before 값 | null
/// }
/// ```
///
/// - 번호가 큰 것(최근)부터다(`pi_store::Store::trade_page`). `before`가 있으면 그 번호보다 작은 매매만.
/// - `max`는 한 쪽의 줄 수: 없으면 [`HISTORY_DEFAULT_MAX`], [`pi_store::HISTORY_PAGE_MAX`](100)보다 크면 100으로
///   줄인다. 1 이상의 정수가 아니면 400 `{"error": "invalid_input"}`.
/// - `next`는 이 쪽 다음에 매매가 더 있을 때만 이 쪽 마지막 줄의 번호이고, 더 없으면 `null`이다. 그 값을
///   `before`로 주면 다음 쪽이 겹침 · 빠짐 없이 이어진다.
/// - `state`는 매매 상태 글자(`discussing` · `judged` · `awaiting_approval` · `blocked` · `ordered` · `holding` ·
///   `sold` · `reviewed`)이고 그 상태인 매매만 낸다. 다른 글자면 400 `{"error": "invalid_input"}`.
/// - `before`가 1 이상의 정수가 아니면 400 `{"error": "invalid_input"}`. 빈 저장소면 빈 목록 · `next` `null`.
/// - 세션이 있어야 열린다(원격은 세션 없이 401).
pub const TRADES_PATH: &str = "/api/trades";

/// 매매 목록 · 승인 기록의 `max`가 없을 때 한 쪽의 줄 수.
pub const HISTORY_DEFAULT_MAX: usize = 20;

/// (29차 1번 · C30 · V83 · F7-7) 매매 하나. `GET /api/trades/{id}` → 200
///
/// ```text
/// {
///   "id", "symbol", "account", "state", "created_at", "updated_at", "decided_at"   (목록의 줄과 같다),
///   "states": [{"state": 바뀐 상태 글자, "from": 바뀌기 전 상태 글자 | null, "at": 유닉스 밀리초}],
///   "side": "buy" | "sell" | null,
///   "quantity": 수량 | null,
///   "facts": [근거 자료 이름],
///   "currency": "KRW" | "USD",
///   "basis": {
///     "hypothesis": 글 | null, "trigger": 글 | null,
///     "entry": 십진수 글자 | null, "stop": 십진수 글자 | null, "target": 십진수 글자 | null,
///     "expected_value": 십진수 글자 | null,
///     "scores": [{"role": 담당, "score": 점수, "reason": 글}],
///     "consensus_score": 점수 | null,
///     "sources": [{"name": 자료 이름, "known_at": 유닉스 밀리초}]
///   },
///   "summary": 쉬운 말 요약
/// }
/// ```
///
/// - `states`는 `pi_store::Store::trade_flow`의 상태 바뀜 줄 그대로다(시각 순서, 만든 줄의 `from`은 `null`).
///   `side` · `quantity` · `facts`도 그 흐름의 계획 방향 · 수량 · 붙은 자료 이름(처음 붙인 순서)이다.
/// - `basis`는 매매의 근거(`pi_store::Trade`의 근거 · 담당 점수 · 합의 점수 · 판단에 쓴 자료)다. 금액은 저장소의
///   통화 최소 단위를 그 시장 통화(`currency`, 설정의 시장)의 십진수 글자로 바꾼 것이다(원화는 그대로, 달러는
///   센트 → 달러). 판단 전이면 빈 칸은 `null` · 빈 목록이다. 글은 가린 것이다.
/// - `summary`는 [`crate::TRADE_BASIS_PATH`]의 `summary`와 같은 글이다(화면은 근거를 따로 부르지 않는다).
/// - 없는 번호면 404 `{"error": "not_found"}`, 번호가 정수가 아니면 400.
/// - 세션이 있어야 열린다(원격은 세션 없이 401).
pub const TRADE_PATH: &str = "/api/trades/{id}";

/// (29차 1번 · C30 · V84) 승인 요청 기록. `GET ?before=&max=&status=` → 200
///
/// ```text
/// {
///   "approvals": [{"id": 줄 번호, "number": 승인 번호, "summary": 가린 사유 글,
///                  "status": "pending" | "approved" | "rejected" | "no_answer" | "expired",
///                  "created_at": 유닉스 밀리초, "resolved_at": 유닉스 밀리초 | null,
///                  "consumed_at": 유닉스 밀리초 | null}],
///   "next": 다음 쪽의 before 값 | null
/// }
/// ```
///
/// - 줄 번호가 큰 것(최근)부터다(`pi_store::Store::approval_page`). `before` · `max` · `next`의 뜻과 400 규칙은
///   [`TRADES_PATH`]와 같다(커서는 줄 번호 `id`다).
/// - `summary`는 승인 카드([`APPROVAL_PATH`])의 `summary`와 같은 글(요청 사유를 가린 것)이다. 비밀 값은 오지 않는다.
/// - `status`가 위 다섯 글자가 아니면 400 `{"error": "invalid_input"}`.
/// - `GET /api/approvals/{id}`(승인 카드) · `POST /api/approvals/answer` · `/api/approvals/{id}/{verb}`는 그대로다.
/// - 세션이 있어야 열린다(원격은 세션 없이 401).
pub const APPROVAL_LOG_PATH: &str = "/api/approvals";

/// 담당이 쓰는 지식 검색 도구의 이름(T16).
pub const KNOWLEDGE_TOOL: &str = "knowledge_search";

/// (10차 10c 14번) 사무실 승인 카드 이벤트(번호 있는 이벤트). 주문이 승인을 기다리면 내용
/// `{"number": 승인 번호, "summary": 가린 내용, "short_code": 지문 앞 6자, "status": "pending"}`을 내고,
/// 답이 나면(폰 · 사무실 어느 쪽이든 [`APPROVAL_DECIDE_PATH`] · [`crate::APPROVAL_ANSWER_PATH`]) 같은 번호로
/// `status`가 `"approved"` · `"rejected"`인 것을 낸다. 승인 · 거절은 폰과 같은 경로 · 같은 지문 대조다.
pub const KIND_OFFICE_APPROVAL: &str = "office.approval";

/// (10차 10c 13번) 뜻 검색 설정. `GET` → `{"enabled", "base_url", "model"}`(처음에는 꺼짐 ·
/// `pi_ai::OLLAMA_DEFAULT_BASE_URL` · [`DEFAULT_EMBED_MODEL`]). `POST` 같은 모양 → 200 같은 모양.
/// 주소가 루프백(`127.0.0.1` · `::1` · `localhost`)이 아니면 400 `{"error": `[`SEMANTIC_LOOPBACK_ONLY`]`}`이고
/// 그대로 둔다. 세션 · Origin 확인을 받고, `POST`는 그 컴퓨터에서만 받는다. 설정은 보관 폴더의
/// [`SEMANTIC_SETTINGS_FILE`]에 남는다(비밀이 아니다).
pub const SETUP_SEMANTIC_PATH: &str = "/api/setup/semantic";

/// 보관 폴더 안 뜻 검색 설정 파일 이름.
pub const SEMANTIC_SETTINGS_FILE: &str = "semantic-settings.json";

/// 뜻 검색 설정 거부: Ollama 주소가 루프백이 아니다.
pub const SEMANTIC_LOOPBACK_ONLY: &str = "loopback_only";

/// 뜻 검색의 기본 임베딩 모델(결정 15번, Ollama `bge-m3`, 1024차원).
pub const DEFAULT_EMBED_MODEL: &str = "bge-m3";

/// 자동 회상으로 꺼낸 지난 매매가 판단 기록의 자료 이름에 붙는 머리. 뒤에 매매 번호가 온다(V22).
pub const RECALL_SOURCE_PREFIX: &str = "지난 매매 #";

/// 웹 푸시 요청 하나의 시간 제한(실제 길).
pub const PUSH_TIMEOUT: Duration = Duration::from_secs(10);

/// VAPID `sub`(연락처) 기본값.
pub const DEFAULT_PUSH_SUBJECT: &str = "mailto:owner@price-inha.invalid";

/// (W5 3차) 백업의 바깥 값. 토큰은 비밀 보관소의 `KeySlot::BackupGithubToken`이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupWiring {
    /// GitHub API 기본 주소(시험은 로컬 가짜 서버).
    pub api_base: String,
    /// 비공개 저장소 `소유자/이름`.
    pub repo: String,
    /// age 수신자 공개키(`age1…`).
    pub recipient: String,
}

/// (W5 3차) 검색 · 백업 · 사무실 · 폰을 잇는 바깥 값. [`App::launch_with`]에 준다.
#[derive(Clone, Default)]
pub struct Reach {
    /// 웹 푸시를 보내는 길. `None`이면 실제 길(`pi_notify::HttpTransport`, [`PUSH_TIMEOUT`]).
    pub push: Option<Arc<dyn pi_notify::PushTransport>>,
    /// 백업. `None`이면 백업하지 않는다(백업 경로 · 하루 정리의 `backup_at`은 `null`).
    pub backup: Option<BackupWiring>,
}
