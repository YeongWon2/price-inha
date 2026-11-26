//! (39차) 메신저(텔레그램)를 잇는다: 토큰 등록 · 페어링 · 받기 루프 · 명령 · 나가는 알림 · 상태.
//!
//! 지시서 `docs/dev/modules/server.md` "39차", 결정 24. 원본 동작 근거: `docs/research/onboarding-messenger.md`
//! 2.4(등록 · 페어링 승인을 서버 쪽에서 · 1:1만) · 2.5(나가는 글 · 명령) · 2.6(승인 — 우리는 메신저로 받지 않는다).
//! 어댑터 · 판정 · 가린 글 · 명령 읽기는 `pi_channel`이 한다. 이 모듈은 잇기만 한다.
//!
//! # 계약 (N2가 따른다)
//!
//! - **조립 값**: [`MessengerWiring`]. 실행 파일 조립([`crate::run_assembly`])은 [`MessengerWiring::default`]
//!   (공식 주소 [`pi_channel::TELEGRAM_API_BASE`])를 [`crate::RunAssembly::messenger`]에 넣는다.
//!   [`crate::App::launch_with_messenger`]가 이 값을 받는다. [`crate::App::launch_with`]는 기본값으로 이것을 부른
//!   것과 같다. 그 밖의 띄우기([`crate::App::start`] · [`crate::App::launch`])는 메신저를 잇지 않는다.
//! - **토큰**: 비밀 보관소(`pi_secrets`)의 공급자 키 [`TELEGRAM_KEYS`](`telegram` · 칸 `bot_token`)에 둔다.
//!   파일 · 기록 · 응답 · 이벤트 · 홈 폴더의 다른 파일에 그대로 나오지 않는다.
//! - **경로**(모두 세션 · 같은 출처, 바꾸는 것은 **같은 컴퓨터에서만** — 원격은 관문이 403):
//!   - `GET` [`MESSENGER_PATH`] → `{"editable": 이 컴퓨터인가, "telegram": {"configured", "bot": 봇 이름|null,
//!     "state": `[`MESSENGER_STATE_OFF`]`|`[`MESSENGER_STATE_POLLING`]`|`[`MESSENGER_STATE_UNAUTHORIZED`]`|`
//!     [`MESSENGER_STATE_ERROR`]`, "paired": 참|거짓, "pending": 기다리는 페어링 수, "locked_until": 유닉스 ms|null}}`.
//!     보낸 사람 ID · 토큰 · 페어링 코드는 싣지 않는다.
//!   - `POST` [`MESSENGER_TELEGRAM_PATH`] `{"token"}`: 저장하기 전에 `Channel::check`(getMe)로 확인한다. 됐으면
//!     저장하고 200 `{"registered": true, "bot": 봇 이름}`, 받기 루프를 (다시) 켠다. 토큰이 거절되면(401) 422
//!     `{"error": "key_rejected"}`, 닿지 못하면 502 `{"error": "unreachable"}`, 몸통이 틀리면 400
//!     `{"error": "invalid_input"}` — 어느 경우든 저장하지 않는다(지금 키 등록의 [`crate::SetupRefusal`] 글자와 같다,
//!     시도 제한도 같은 규칙). 시연 모드(`settings.demo`)면 확인 없이 409 `{"error": "demo_mode", "message":
//!     `[`MESSENGER_DEMO_REFUSED`]`}`.
//!   - `DELETE` [`MESSENGER_TELEGRAM_PATH`]: 토큰과 페어링 장부([`MESSENGER_BOOK_FILE`]) · 받은 자리
//!     ([`MESSENGER_CURSOR_FILE`])를 지우고 루프를 멈춘다. 200 `{"removed": true, "notice": `[`MESSENGER_REVOKE_HINT`]`}`
//!     (다듬기 E 7번: 텔레그램 쪽 토큰은 살아 있으니 BotFather에서 폐기하라는 안내).
//!   - `POST` [`MESSENGER_PAIRING_APPROVE_PATH`] `{"code"}`: `Gatekeeper::approve`. 맞으면 200 `{"paired": true}`.
//!     틀리면 400 `{"error": "wrong_code", "message": `[`MESSENGER_WRONG_CODE`]`}`(남은 횟수를 싣지 않는다).
//!     잠겼으면(5번째 틀림부터) 423 `{"error": "locked", "message": `[`MESSENGER_LOCKED`]`, "locked_until": 유닉스 ms}`.
//!     이미 허용된 사람이 있으면 409 `{"error": "owner_exists", "message": `[`MESSENGER_OWNER_EXISTS`]`}`.
//!   - `POST` [`MESSENGER_PAIRING_REVOKE_PATH`](몸통 없음): 허용된 사람을 뺀다. 200 `{"revoked": 뺐는가}`.
//! - **이벤트** [`KIND_MESSENGER`] `{"action", "channel": "telegram", "sender_tail"?}`: 토큰 저장
//!   [`MESSENGER_ACTION_TOKEN_SET`] · 지움 [`MESSENGER_ACTION_TOKEN_REMOVED`] · 승인 [`MESSENGER_ACTION_PAIRED`] ·
//!   빼기 [`MESSENGER_ACTION_REVOKED`]. `sender_tail`(승인 · 빼기만)은 보낸 사람 ID의 뒤 4자리뿐이다.
//! - **받기 루프**(`messenger/runner.rs`): 토큰이 있고 시연 모드가 아니면 long polling을 돈다(띄울 때 · 등록할 때).
//!   받은 자리(`Cursor`)는 홈 폴더의 [`MESSENGER_CURSOR_FILE`]에 두어 다시 켜도 이어 받는다 — 받은 묶음을 처리한 뒤
//!   다음 받기 전에 쓴다(같은 글을 두 번 처리하지 않는다). 받은 글마다 `Gatekeeper::judge`(시각은 `Wiring::clock`): `PairingIssued`면 그 대화에
//!   [`MESSENGER_PAIRING_REPLY`] 뒤에 코드를 붙여 답하고, `PairingPending` · `Ignored`면 답하지 않는다. `Allowed`면
//!   명령으로 읽는다. 오류: `RateLimited`는 `retry_after_secs`초를, `Transport` · `Api`(5xx 등) · `Malformed`는
//!   [`MessengerWiring::backoff_initial`]부터 실패마다 두 배로 [`MessengerWiring::backoff_max`]까지 물러서며 다시
//!   한다 — 쉬기 · 물러서기는 모두 `Wiring::clock`의 `sleep`으로 잔다(시험이 시계로 본다). 받기에 성공하면 물러서기를
//!   처음으로 돌린다. `Unauthorized`는 루프를 멈추고 상태를 [`MESSENGER_STATE_UNAUTHORIZED`]로(알림 한 번).
//! - **페어링 장부**(`messenger/book.rs`): 홈 폴더의 [`MESSENGER_BOOK_FILE`](0600, 원자적 쓰기)로 `PairingBook`을
//!   구현한다. 저장소 판을 올리지 않는다.
//! - **명령**(`messenger/commands.rs`, 허용된 사람의 1:1 글만 — `pi_channel::parse_command`):
//!   - `/status` → 한 덩이: 팀 상태(계좌 화면과 같은 글: "운용 중" · "쉼" · "하루 한도 멈춤" · "이상 감지 멈춤" ·
//!     "전체 정지") · 오늘 손익 · `보유 <수>` · [`MESSENGER_STATUS_PENDING`]` <기다리는 승인 수>`.
//!   - `/approvals` → 기다리는 승인마다 `<짧은 번호> <한 줄 요약(승인 요청 알림의 요약 — 종목 번호가 든다)>`, 끝에 [`MESSENGER_APPROVE_ON_WEB_LINK`] 뒤에
//!     링크(또는 `price-inha open`). 없으면 [`MESSENGER_NO_APPROVALS`]. 짧은 번호는 승인 번호의 앞
//!     [`MESSENGER_SHORT_ID_LEN`]자다.
//!   - `/reject <짧은 번호>` → 지금 승인 길의 거절과 같은 일(사무실 승인 이벤트가 answered가 된다, 메신저가 했다고
//!     남긴다). 답 [`MESSENGER_REJECTED`]. 그 번호의 기다리는 승인이 없으면 [`MESSENGER_NO_SUCH_APPROVAL`],
//!     이미 끝났으면 [`MESSENGER_APPROVAL_DONE`].
//!   - (40차 3번 · 결정 25) `/stop` → [`crate::TEAM_STOP_PATH`]와 같은 길로 전체 정지한다(사유 `OwnerStop`, 내 승인
//!     표시). 답 [`MESSENGER_STOPPED`]. 이미 전체 정지면 팀 상태 · 기록을 그대로 두고 [`MESSENGER_ALREADY_STOPPED`].
//!     메신저로 다시 시작은 여전히 안 된다(웹에서만, 결정 20 · 24).
//!   - `/help` → [`MESSENGER_HELP`](40차 3번: `/stop`이 다시 든다 — `/status` · `/approvals` · `/reject` · `/stop` ·
//!     `/help`). 승인을 뜻하는 글(`ApproveNotHere`) → [`MESSENGER_APPROVE_ON_WEB`](승인 상태는
//!     그대로). 그 밖 → [`MESSENGER_UNKNOWN`].
//!   - 메신저로는 승인 · 재개 · 규칙 변경 · 실전 승격을 하지 않는다. 계산 · 요약은 서버가 한다.
//! - **나가는 알림**(`messenger/outbound.rs`): 폰 푸시로 가는 알림(`pi_notify::NoticeKind` 모두)을 허용된 사람의
//!   대화에도 보낸다. (40차 4 · 5 · 7번) 종류별로 메신저를 끈 알림 · 조용한 시간에 미루는 급하지 않은 알림은 보내지
//!   않는다 — 판단은 알림이 나가는 한곳(`Linked::deliver`)이 [`crate::NOTIFY_SETTINGS_PATH`]의 설정으로 한다. 글은 그 알림의 제목 · 본문이고 `OutText::redacted`(가리개는 비밀 보관소)를 거친다 — 가리기에
//!   실패하면 보내지 않고 기록만. 승인 요청 알림에는 [`MESSENGER_REJECT_HINT`]` <짧은 번호>`[`MESSENGER_WEB_ONLY`]를
//!   붙인다. 허용된 사람이 없으면 보내지 않는다. 보내기 실패는 팀의 일을 막지 않는다(물러서며
//!   [`MessengerWiring::send_attempts`]번까지, 그래도 안 되면 버리고 기록).
//! - **기록**: HTTP 라이브러리의 자세한 연결 기록을 켜지 않고 그 기록 대상을 debug 아래로 둔다(주소에 토큰이 든다).
//! - **시연 모드**(`settings.demo`): 루프를 돌리지 않고 바깥(텔레그램)으로 요청을 보내지 않는다.

mod book;
mod commands;
mod http;
mod hub;
mod outbound;
mod runner;
mod status;

use std::time::Duration;

use pi_secrets::ProviderKeys;

pub use book::{MESSENGER_BOOK_FILE, MESSENGER_CURSOR_FILE};
pub use commands::{
    MESSENGER_ALREADY_STOPPED, MESSENGER_APPROVAL_DONE, MESSENGER_APPROVE_ON_WEB,
    MESSENGER_APPROVE_ON_WEB_LINK, MESSENGER_HELP, MESSENGER_NO_APPROVALS,
    MESSENGER_NO_SUCH_APPROVAL, MESSENGER_REJECTED, MESSENGER_SHORT_ID_LEN,
    MESSENGER_STATUS_PENDING, MESSENGER_STOPPED, MESSENGER_UNKNOWN,
};
pub use http::MESSENGER_REVOKE_HINT;
pub(crate) use http::routes;
pub use http::{
    MESSENGER_DEMO_REFUSED, MESSENGER_LOCKED, MESSENGER_OWNER_EXISTS,
    MESSENGER_PAIRING_APPROVE_PATH, MESSENGER_PAIRING_REVOKE_PATH, MESSENGER_PATH,
    MESSENGER_TELEGRAM_PATH, MESSENGER_WRONG_CODE,
};
pub(crate) use hub::{Messenger, MessengerParts};
pub use outbound::{MESSENGER_REJECT_HINT, MESSENGER_WEB_ONLY};
pub use runner::MESSENGER_PAIRING_REPLY;
pub use status::{
    KIND_MESSENGER, MESSENGER_ACTION_PAIRED, MESSENGER_ACTION_REVOKED,
    MESSENGER_ACTION_TOKEN_REMOVED, MESSENGER_ACTION_TOKEN_SET, MESSENGER_STATE_ERROR,
    MESSENGER_STATE_OFF, MESSENGER_STATE_POLLING, MESSENGER_STATE_UNAUTHORIZED,
};

/// 봇 토큰을 두는 비밀 보관소의 공급자 키(`telegram`, 칸 `bot_token`).
pub const TELEGRAM_KEYS: ProviderKeys = ProviderKeys::new("telegram", &["bot_token"]);

/// 메신저 조립 값. 시험은 가짜 텔레그램 서버 주소와 짧은 시간을 넣는다.
#[derive(Debug, Clone)]
pub struct MessengerWiring {
    /// 텔레그램 API 기준 주소(실행 파일은 [`pi_channel::TELEGRAM_API_BASE`]). 주소로 읽을 수 없으면 루프를 켜지
    /// 않고 상태는 [`MESSENGER_STATE_ERROR`]다.
    pub telegram_api_base: String,
    /// long polling 한 번의 `timeout`(초).
    pub poll_timeout_secs: u32,
    /// 요청 하나의 상한(long polling은 그 위에 `poll_timeout_secs`를 더한다).
    pub request_timeout: Duration,
    /// 받기 실패(연결 · 5xx · 읽을 수 없는 답) 뒤 첫 물러서기. 실패마다 두 배.
    pub backoff_initial: Duration,
    /// 물러서기의 가장 긴 간격.
    pub backoff_max: Duration,
    /// 나가는 알림 하나를 보내 보는 가장 많은 횟수(처음 포함).
    pub send_attempts: u32,
}

impl Default for MessengerWiring {
    /// 공식 주소 · long polling 25초 · 요청 10초 · 물러서기 1초부터 60초까지 · 보내기 3번.
    fn default() -> Self {
        Self {
            telegram_api_base: pi_channel::TELEGRAM_API_BASE.to_string(),
            poll_timeout_secs: 25,
            request_timeout: Duration::from_secs(10),
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            send_attempts: 3,
        }
    }
}
