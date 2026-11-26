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
mod history_http;
mod linked;
mod looks_http;
mod me_http;
mod memo_http;
mod notice_http;
mod office_bridge;
mod office_gather;
mod office_http;
mod office_work;
mod phone_http;
mod semantic;
mod semantic_http;
mod sentence;
mod summary;
mod summon_http;
mod summon_sink;
mod talk_feed;
mod team_chat;
// (51차) 논의 진행 단계 · 하루 AI 잔량 표시판.
mod progress_board;

pub(crate) use assets::routes as asset_routes;
pub(crate) use backup_http::GITHUB_API_BASE;
pub use backup_http::{BACKUP_SETTINGS_FILE, SETUP_BACKUP_PATH, restore_backup};
pub(crate) use backup_http::{clear_restore_leftover, routes as backup_routes};
pub(crate) use linked::{Linked, LockNotices};
pub(crate) use looks_http::{Looks, routes as looks_routes};
pub(crate) use me_http::{Me, routes as me_routes};
pub use memo_http::{KIND_OFFICE_MEMO, MEMOS_DEFAULT_DAYS, MEMOS_MAX_DAYS, MEMOS_PATH};
pub(crate) use office_bridge::BridgeTurn;
pub(crate) use office_http::{OfficeDesk, routes as office_routes};
pub(crate) use phone_http::{Phone, routes as phone_routes};
pub(crate) use progress_board::ProgressBoard;
pub(crate) use semantic::{Semantic, SemanticSettings};
pub(crate) use semantic_http::routes as semantic_routes;
pub(crate) use summary::daily_summary;
pub(crate) use summon_sink::{SpareBudget, SummonSink};
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
/// (34차 1번, C27 · E49) [`OFFICE_PENDING_APPROVALS`] 칸에 아직 기다리는 승인 요청을 싣는다(다시 열어도 ✋를 다시 띄운다).
pub const OFFICE_PATH: &str = "/api/office";

/// (34차 1번, C27 · E49 · V62) [`OFFICE_PATH`] 스냅샷의 기다리는 승인 칸 이름.
///
/// 값은 결과가 기다림(`pending`)인 승인 요청만의 목록이고, 만든 시각이 이른 것부터다(같으면 번호 순).
/// 한 줄은 정확히 `{"agent", "number", "created_at", "summary", "short_code"}`이다.
///
/// - `agent`: ✋를 띄울 담당 id(`pi_office::Agent::id`). 주문 → `"trader"`(매매 담당), 규칙 개선안 →
///   `"review"`(복기 담당), 실전 승격 · 다시 시작 → `"leader"`(팀장).
/// - `number`: 승인 번호(`pi_approval::ApprovalNumber` 글자). `created_at`: 만든 시각(유닉스 밀리초).
/// - `summary`: 승인 카드와 같은 가린 사유 글(비밀 값은 가린다). `short_code`: 지문 앞 6자. 전체 지문은 싣지 않는다.
/// - 승인 · 거절 · 답 없음 · 만료로 닫힌 요청은 싣지 않는다. 기다리는 것이 없으면 빈 목록(`[]`)이다.
pub const OFFICE_PENDING_APPROVALS: &str = "pending_approvals";

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
/// - (33차 1번, C18) `room`이 `"team"`이고 `agent`가 없으면 멘션 글이다: 글 속 `@[이름]`([`OFFICE_ROSTER`]의 이름과
///   정확히 같은 것)으로 멘션된 담당만 차례로 답한다. 200 `{"responders": [담당 id…], "responses": [응답 객체…]}`.
///   멘션이 없으면 `{"responders": [], "hint": `[`OFFICE_HINT_NO_MENTION`]`}`, 이름이 맞지 않으면
///   `{"responders": [], "hint": `[`OFFICE_HINT_NO_MATCH`]`}`이고 AI를 부르지 않는다. 멘션된 담당은 답 턴이 시작되는
///   순간 [`KIND_OFFICE_SUMMON`] `called`(`reason: "mention"`)로 곁에 온다(논의 중이면 오지 않고 답만 간다).
///   `agent`를 주면 지금처럼 그 담당 하나다(응답 모양 그대로). 1:1 방(`direct`)은 `agent`가 있어야 한다.
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

/// (32차 1번, C17 · M8) 내 캐릭터. 사용자가 한 명이라 하나뿐이다. 저장은 저장소의 `player_profile`
/// (`pi_store::Store::player` · `set_player_profile` · `set_player_spot`)에 한다 — 서버를 다시 켜도 남고 백업에 따라간다.
///
/// - `GET` → 200 `{"name", "look", "x", "y", "dir"}`. 정한 적이 없으면 이름 [`pi_store::PLAYER_DEFAULT_NAME`],
///   외형 [`PLAYER_DEFAULT_LOOK`], 저장된 자리가 없으면 입구 [`OFFICE_ENTRANCE`] · [`OFFICE_ENTRANCE_DIR`].
///   `dir`는 `"up"` · `"down"` · `"left"` · `"right"`.
/// - `POST` `{"name"?, "look"?}` → 200 `GET`과 같은 모양. 빠진 칸은 지금 값 그대로다. 이름의 한도는 저장소가 정한 그대로
///   (1~[`pi_store::PLAYER_NAME_MAX_CHARS`]자 — server는 한도를 새로 정하지 않고 저장소의 거부를 400으로 옮긴다).
///   외형은 원본 외형 목록([`OFFICE_LOOKS_PATH`]의 `available`)에 있어야 한다. 틀리면(글이 아님 포함) 400
///   `{"error": `[`OFFICE_BAD_INPUT`]`}`이고 아무것도 저장하지 않는다(E3). 자리는 그대로다.
/// - 관문은 사무실의 다른 API와 같다: 세션이 없으면(원격 포함) 401, 쓰기는 같은 출처(`Origin`)만.
/// - 두 화면이 따로 남기면 마지막에 온 것이 이긴다. 다른 화면에 중계하지 않는다(E59).
pub const OFFICE_ME_PATH: &str = "/api/office/me";

/// (32차 1번 · 2번, C17) 내 캐릭터 자리 남기기. `POST` `{"x", "y", "dir"}` → 200 [`OFFICE_ME_PATH`]의 `GET`과 같은 모양.
/// `x`는 0 이상 [`OFFICE_COLS`] 미만, `y`는 0 이상 [`OFFICE_ROWS`] 미만의 정수, `dir`는 방향 넷 가운데 하나다.
/// 벗어나거나 빠지거나 모양이 틀리면 400 `{"error": `[`OFFICE_BAD_INPUT`]`}`이고 저장하지 않는다(E3). 이름 · 외형은
/// 그대로다. 화면은 걷는 동안 매 칸 보내지 않고 1초에 많아야 한 번 · 멈춘 뒤 · 떠날 때 보낸다. 관문은 [`OFFICE_ME_PATH`]와 같다.
pub const OFFICE_ME_POSITION_PATH: &str = "/api/office/me/position";

/// 사무실 지도의 칸 수(가로). 화면 지도 `web/public/office/office-map.json`의 `cols`와 같다.
pub const OFFICE_COLS: u32 = 44;
/// 사무실 지도의 칸 수(세로). 화면 지도의 `rows`와 같다.
pub const OFFICE_ROWS: u32 = 30;
/// 내 캐릭터의 기본 자리(입구, 칸 `(x, y)`): 화면 지도 `entrance`의 가운데 열, 입구 줄 바로 안쪽(한 줄 위).
/// 화면 `office/pathfind.ts`의 `entranceCell`과 같다.
pub const OFFICE_ENTRANCE: (u32, u32) = (37, 28);
/// 입구에서 바라보는 방향(사무실 안쪽).
pub const OFFICE_ENTRANCE_DIR: &str = "up";
/// 내 캐릭터의 기본 외형(원본 DeskRPG 기본 외형 `office-jun`, 원본 외형 목록 안).
pub const PLAYER_DEFAULT_LOOK: &str = "office-jun";

/// (22차 1번) 사무실 팀 대화의 최근 줄. `GET` → 200 `[{"room", "agent", "text", "at", "topic"}, …]`: 저장된
/// [`KIND_OFFICE_TALK`] 이벤트 가운데 최근 [`OFFICE_TALK_RECENT`]줄을 시각 순서(오래된 것 먼저)로 준다. 이벤트 내용
/// 그대로다. 세션이 있어야 열린다(없으면 401).
pub const OFFICE_TALK_PATH: &str = "/api/office/talk";

/// [`OFFICE_TALK_PATH`]가 주는 최근 줄 수.
pub const OFFICE_TALK_RECENT: usize = 50;

// ───────────── (33차) 멘션 · 전체 공지 · 담당 부르기 (C18 · C19 · C20, 결정 17 · 19) ─────────────
//
// 아래 크레이트가 이미 주는 길을 잇는다: 멘션 `pi_office::Office::post_team` · `complete_with_relay`(명단은
// [`OFFICE_ROSTER`]), 공지 `post_notice`(저장소 `pi_store` 공지 · 확인, 가리기 · 하루 상한 포트를 끼운다),
// 부르기 `summon` · `summon_arrived` · `summon_returned` · `mention_turn_started` · `user_left_room` · `tick`.
// 관문은 사무실의 다른 쓰기와 같다(세션이 없으면 원격 포함 401, 쓰기는 같은 출처만). 글은 저장 · 이벤트 전에 가린다.

/// (33차 1번, C18) 사무실 담당의 표시 이름(멘션 `@[이름]`이 정확히 맞아야 하는 이름). 화면 지도
/// `web/public/office/office-map.json`의 `agents.<id>.name`과 같다. 순서는 `pi_office::Agent::ALL`.
pub const OFFICE_ROSTER: [(&str, &str); 6] = [
    ("news", "뉴스 공시 시세"),
    ("strategy", "전략"),
    ("risk", "리스크"),
    ("review", "복기"),
    ("trader", "매매"),
    ("leader", "팀장"),
];

/// (33차 1번, C18 · E44) 팀 방 글에 멘션이 없어 아무도 답하지 않을 때의 `hint`(`pi_office::TeamHint::NoMention`).
/// 응답 `{"responders": [], "hint": "no_mention"}`. 글은 [`KIND_OFFICE_MESSAGE`]로 남고 AI는 부르지 않는다.
pub const OFFICE_HINT_NO_MENTION: &str = "no_mention";

/// (33차 1번, E43) 멘션한 이름이 명단과 하나도 맞지 않을 때의 `hint`(`pi_office::TeamHint::NoMatch`).
/// 응답 `{"responders": [], "hint": "no_match"}` — 보낸 사람에게만 가는 답이다(이벤트로 다른 화면에 알리지 않는다).
/// 화면은 [`OFFICE_NO_MATCH_TEXT`]를 보인다. AI는 부르지 않는다.
pub const OFFICE_HINT_NO_MATCH: &str = "no_match";

/// (33차 1번, E43) 멘션한 상대가 없을 때 보낸 사람에게만 보이는 안내 글(원본 DeskRPG 글 그대로).
pub const OFFICE_NO_MATCH_TEXT: &str = "지목한 상대가 이 방에 없어 아무도 답하지 않았습니다.";

/// (33차 3번, C19 · F11 · M10 · N18) 전체 공지. `POST` `{"text", "sourceMessageId"}` → 200
/// `{"notice": 번호, "acks": [{"agent": 담당 id, "state": "queued" | "limit"}]}`(담당 여섯, `pi_office::Agent::ALL` 순서).
///
/// - 글을 가린 뒤 저장소에 공지로 남기고(담당 여섯 모두 기다림) `office.notice` `{"notice", "text", "at"}`를 낸다.
///   담당마다 한 줄 확인 답 턴을 대기열에 넣는다(`office.chat`에 `"notice": 번호`). 답이 끝나면 확인은 "답함",
///   실패면 "실패"로 한 번만 바뀌고 `office.notice_ack` `{"notice", "agent", "state"}`를 낸다.
/// - 하루 AI 호출 상한(팀장 예비분 제외)에 걸린 담당은 턴을 받지 않고 `"limit"`이다(E45). 공지 자체는 저장된다.
/// - 같은 `sourceMessageId`를 다시 보내면 처음 답을 그대로 돌려주고 공지 · 턴을 다시 만들지 않는다.
/// - 글 · 번호의 한도와 보내기 간격은 [`OFFICE_CHAT_PATH`]와 같다(빈 글 400 [`OFFICE_BAD_INPUT`],
///   [`OFFICE_TEXT_MAX`]자 넘음 400 [`OFFICE_TOO_LONG`], 같은 기기 [`OFFICE_SEND_INTERVAL`] 안 429 [`OFFICE_TOO_FAST`]).
/// - 공지는 규칙 · 템플릿 값 · 승인 · 팀 상태를 바꾸지 않는다(E46 · V74).
/// - 공지는 그 뒤 열리는 논의의 담당 자료로 넘어간다(`pi_agent::DiscussionOptions::with_notices`). 보낸 시각이
///   그 논의의 판단 시각보다 늦은 공지는 넣지 않는다(결정 19).
pub const OFFICE_NOTICE_PATH: &str = "/api/office/notice";

/// (33차 3번, C19) 지난 공지와 확인 상태. `GET ?limit=<수>` → 200 `[{"notice": 번호, "text": 가린 글,
/// "at": 보낸 시각(유닉스 밀리초), "acks": [{"agent": 담당 id, "state": "waiting" | "answered" | "limit" | "failed"}]}]`
/// (최근 것 먼저). `limit`이 없으면 [`OFFICE_NOTICES_DEFAULT`]개, 수가 아니면 400 [`OFFICE_BAD_INPUT`].
/// 세션이 있어야 열린다(없으면 401).
pub const OFFICE_NOTICES_PATH: &str = "/api/office/notices";

/// [`OFFICE_NOTICES_PATH`]가 `limit` 없이 주는 공지 수.
pub const OFFICE_NOTICES_DEFAULT: usize = 20;

/// (33차 4번, C20) 담당 부르기. `POST` `{"agent": 담당 id}` → 200 `{"agent", "phase": "called"}`.
///
/// - 모르는 담당(빠짐 포함)이면 400 `{"error": `[`OFFICE_UNKNOWN_AGENT`]`}`.
/// - 담당이 미팅룸 논의 중이면 409 `{"error": `[`OFFICE_MEETING_RESERVED`]`}`이고 아무것도 바뀌지 않는다(E47).
/// - 단계가 바뀔 때마다 [`KIND_OFFICE_SUMMON`]을 낸다.
pub const OFFICE_SUMMON_PATH: &str = "/api/office/summon";

/// (33차 4번, C20) 화면이 "걸어온 담당이 도착했다"를 알린다. `POST` `{"agent"}` → 200 `{"agent", "phase"}`(알린 뒤 단계).
/// 오는 중(`called`)이면 곁에서 기다림(`waiting`), 돌아가는 중(`returning`)이면 자리(`idle`, 원본 `npc:arrived`가
/// 두 경우 모두 같은 알림이다). 그 밖의 단계면 그대로다. 모르는 담당이면 400 [`OFFICE_UNKNOWN_AGENT`].
pub const OFFICE_SUMMON_ARRIVED_PATH: &str = "/api/office/summon/arrived";

/// (33차 2번, C20) 화면이 지금 보는 방을 알린다. `POST` `{"room": "team" | "direct"}` → 200 `{"room"}`.
/// 팀 방이 아닌 방으로 바꾸면 멘션으로 온 담당은 바로 돌아간다(`returning`, 원본 — 타이머 없음). 부름으로 온 담당은
/// 그대로다. 모르는 방이면 400 [`OFFICE_BAD_INPUT`].
pub const OFFICE_SUMMON_VIEW_PATH: &str = "/api/office/summon/view";

/// (33차 4번, C20) 부르기 단계 이벤트(번호 있는 이벤트). 내용 `{"agent": 담당 id, "phase": "called" | "waiting" |
/// "returning" | "idle", "reason": "call" | "mention" | null, "at": 바뀐 시각(유닉스 밀리초)}`.
/// 사무실 크레이트가 단계를 실어 내는 `office.agent`(칸 `phase` · `reason` · `phaseAt`)는 그대로 두고, 단계가 바뀐
/// 담당마다 이것을 하나씩 더 낸다. 멘션된 담당은 답 턴이 시작되는 순간 `called`(`reason: "mention"`)이다.
pub const KIND_OFFICE_SUMMON: &str = "office.summon";

/// (33차 4번, C20) 모르는 담당을 부를 때의 오류 이름.
pub const OFFICE_UNKNOWN_AGENT: &str = "unknown_agent";

/// (33차 4번, E47) 논의 중인 담당을 부를 때의 오류 이름(409).
pub const OFFICE_MEETING_RESERVED: &str = "meeting_reserved";

/// (33차 4번, N10) 서버가 사무실 시계를 굴리는 간격(`pi_office::Office::tick`). 깨우기 엔진과 따로, 가볍게 돈다.
/// 곁에서 10초(`pi_office::SUMMON_IDLE_MS`) · 도착 없음 30초(`pi_office::SUMMON_ARRIVE_MS`)는 넣어 준 시계로 재고,
/// 이 간격은 "때가 됐는지 보는" 벽시계 간격이다.
pub const OFFICE_SUMMON_TICK: Duration = Duration::from_millis(250);

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
/// "sourceMessageId": 글 번호}`. (41차 3번) 메신저에서 온 글은 `"via": `[`crate::MESSENGER_VIA`]가 더 붙는다
/// (사무실 채팅이 "메신저에서" 표시를 단다). 화면에서 온 글에는 `via`가 없다.
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
///
/// (34차 1번, C27) 모든 승인 카드 이벤트에 두 칸을 더한다(옛 칸은 그대로):
/// - `"agent"`: ✋를 띄울 담당 id. 주문 → `"trader"`, 규칙 개선안 → `"review"`, 실전 승격 · 다시 시작 → `"leader"`
///   ([`OFFICE_PENDING_APPROVALS`]와 같은 규칙).
/// - `"state"`: `"waiting"`(기다림) · `"answered"`(승인 · 거절) · `"expired"`(답 없음 · 만료로 닫힘).
///   답 없음 · 만료로 닫힐 때도 같은 번호로 이 이벤트를 낸다.
///
/// (35차 5번) 규칙 개선안(복기가 낸 것 · 실전중 템플릿의 위험을 키우는 사용자 변경)의 승인 요청도 다른 승인과
/// 같게 만들 때 이 이벤트(`agent` `"review"`, `state` `"waiting"`, `status` `"pending"`)를 한 번 낸다
/// (`crate::KIND_PROPOSAL`은 그대로 낸다). 폰 알림은 지금 규칙 그대로다(두 번 가지 않게).
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
