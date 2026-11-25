//! (40차 4 · 5 · 6 · 7번, 결정 24) 알림 전달처 설정과 조용한 시간([`NOTIFY_SETTINGS_PATH`]).
//!
//! 원본 동작 근거: `docs/research/onboarding-messenger.md` 2.5(원본의 종류별 전달 · 조용한 시간). 끌 수 없는 종류는
//! 결정 24 · 25(우리 것)다. 계산 · 검사는 서버가 하고 화면(설정 화면 "알림" 묶음)은 그린다.
//!
//! # 계약 (N2가 따른다 — 40차 N1 뼈대에는 아직 경로가 없다)
//!
//! ## 읽기 (`GET` [`NOTIFY_SETTINGS_PATH`], 세션, 원격 세션도 읽는다, 200)
//!
//! ```json
//! {
//!   "kinds": [{"kind": "approval_request", "label": "승인 요청", "push": true, "messenger": true, "locked": true}, …],
//!   "quiet": {"enabled": false, "start": "22:00", "end": "07:00"},
//!   "editable": true
//! }
//! ```
//!
//! - `kinds`는 `pi_notify::NoticeKind::ALL`의 차례 그대로 여섯 줄이다. `kind`는 `NoticeKind::name`, `label`은
//!   [`NOTIFY_KIND_LABELS`]의 글, `locked`는 [`NOTIFY_LOCKED_KINDS`]에 들면 참이다.
//! - `quiet.start` · `quiet.end`는 시장 시간대(국내는 한국 시각)의 `HH:MM`(두 자리 시 00~23 · 두 자리 분 00~59)이다.
//! - `editable`은 이 컴퓨터에서 온 요청이면 참, 다른 기기면 거짓(`pi_gate::local_only`와 같은 판단).
//! - 기본(저장한 값이 없거나 설정 파일을 읽지 못함 · 깨짐): 모든 종류의 `push` · `messenger`가 참, 조용한 시간은
//!   꺼짐(`enabled` 거짓, 시각은 [`NOTIFY_QUIET_DEFAULT_START`] · [`NOTIFY_QUIET_DEFAULT_END`]) — 지금과 같다.
//!
//! ## 바꾸기 (`POST` [`NOTIFY_SETTINGS_PATH`], 세션 · 같은 출처 · **이 컴퓨터에서만** — 원격 세션은 403)
//!
//! 몸통은 읽기와 같은 모양의 `kinds` · `quiet`이다(통째로 바꾼다). `label` · `locked` · `editable`은 받지 않아도 되고
//! 와도 무시한다. 답은 200 읽기와 같은 모양(저장한 뒤의 값)이다.
//!
//! - `kinds`에는 여섯 종류가 한 번씩 모두 있어야 한다. 모르는 종류 · 빠진 종류 · 겹친 종류 · 참/거짓이 아닌 값 ·
//!   JSON 아님은 400 `{"error": "invalid_input"}`.
//! - 잠긴 종류([`NOTIFY_LOCKED_KINDS`])의 `push` · `messenger`를 둘 다 끄면 400
//!   `{"error": `[`NOTIFY_LOCKED_OFF`]`, "kind": 그 종류}` — 적어도 한 전달처로는 간다.
//! - `quiet.start` · `quiet.end`가 `HH:MM`이 아니거나(예: `"25:00"` · `"12:60"` · `"7시"` · 빈 글자), 켰는데 시작과
//!   끝이 같으면(`pi_notify::QuietHours::new`가 받지 않는다) 400 `{"error": `[`NOTIFY_INVALID_QUIET`]`}`.
//! - 400 · 403이면 아무것도 저장하지 않는다(값은 그대로다).
//! - 저장은 보관 폴더(홈)의 [`NOTIFY_SETTINGS_FILE`]에 원자적으로(임시 파일 → 이름 바꾸기) 한다. 비밀이 아니다.
//!   저장소 판을 올리지 않는다. 같은 보관 폴더로 다시 띄워도 남는다.
//!
//! ## 전달처 고르기 (알림이 나가는 한곳 `Linked::deliver`)
//!
//! - 그 종류의 `push`가 거짓이면 폰 푸시를 보내지 않는다. `messenger`가 거짓이면 메신저로 보내지 않는다.
//! - 조용한 시간을 켰고 지금(시계 포트)이 그 시간대(시작 포함 · 끝 제외, 자정을 넘을 수 있다)에 들면, 급하지 않은
//!   알림(`NoticeKind::ignores_quiet_hours`가 거짓 — 지금은 백업 실패 · 하루 정리)은 notify의 지금 규칙대로 폰 푸시를
//!   미루고 메신저로도 보내지 않는다. 급한 알림(승인 요청 · 멈춤 · 이상 · 접속 잠금)은 그대로 간다.
//!   조용한 시간은 `pi_notify::Notifier::set_quiet_hours`로 바로 바꾼다(다시 띄우지 않아도 된다).
//! - 미룬 푸시를 조용한 시간이 끝난 뒤 내보내는 것은 notify가 지금 하는 만큼(`release_held` — 폰 푸시만)이다.
//!   메신저로 미룬 글을 나중에 내보내는 길은 새로 만들지 않는다.
//! - 설정을 읽지 못하면 기본(모두 보냄)으로 — 알림이 조용히 사라지지 않게.
//!
//! ## (41차 2번, 결정 26) 팀 논의 종류 [`NOTIFY_TEAM_TALK_KIND`]
//!
//! - 읽기의 `kinds` 맨 끝(여섯 `NoticeKind` 줄 다음)에 한 줄 더 있다:
//!   `{"kind": "team_talk", "label": "팀 논의", "push": null, "messenger": 참|거짓, "locked": false}`. `push`가 `null`인
//!   것은 폰 칸이 없다는 뜻이다(논의 글은 폰 푸시로 가지 않는다). 기본 `messenger`는 참(켜짐). 잠긴 종류가 아니다.
//! - 바꾸기 몸통에서 이 줄은 **있어도 되고 없어도 된다**: 있으면 그 `messenger`(참/거짓이 아니면 400 `invalid_input`)를
//!   저장하고 `push`는 무엇이 와도 보지 않는다. 없으면 지금 값 그대로다. 여섯 `NoticeKind` 줄의 규칙은 그대로다.
//! - [`NOTIFY_KIND_LABELS`] · [`NOTIFY_LOCKED_KINDS`]는 `NoticeKind` 여섯 그대로다(이 종류는 따로 둔다).
//! - 조용한 시간에 든 동안의 논의 글은 메신저로 보내지 않고 버린다(`crate::messenger` 문서).

/// (41차 2번) 팀 논의 글(메신저로 흐르는 `office.talk`)의 알림 설정 종류 글자. 폰 칸은 없다.
pub const NOTIFY_TEAM_TALK_KIND: &str = "team_talk";

/// (41차 2번) [`NOTIFY_TEAM_TALK_KIND`]의 화면 글.
pub const NOTIFY_TEAM_TALK_LABEL: &str = "팀 논의";

/// 알림 전달처 설정을 읽고(`GET`) 통째로 바꾸는(`POST`, 이 컴퓨터에서만) 경로.
pub const NOTIFY_SETTINGS_PATH: &str = "/api/notify/settings";

/// 저장한 설정을 두는 파일 이름(보관 폴더 안). 비밀이 아니다.
pub const NOTIFY_SETTINGS_FILE: &str = "notify-settings.json";

/// 끌 수 없는 종류(`NoticeKind::name`): 승인 요청 · 멈춤(전체 정지 등) · 이상(이상 감지 멈춤 · 손절을 다시 내지
/// 못함 등). 적어도 한 전달처(폰 푸시 · 메신저)로는 간다.
pub const NOTIFY_LOCKED_KINDS: [&str; 3] = ["approval_request", "stall", "anomaly"];

/// 종류(`NoticeKind::name`)와 화면 글. `pi_notify::NoticeKind::ALL`의 차례다.
pub const NOTIFY_KIND_LABELS: [(&str, &str); 6] = [
    ("approval_request", "승인 요청"),
    ("stall", "멈춤"),
    ("anomaly", "이상"),
    ("access_lock", "접속 잠금"),
    ("backup_failure", "백업 실패"),
    ("daily_summary", "하루 정리"),
];

/// 조용한 시간의 기본 시작(꺼져 있을 때 보이는 값).
pub const NOTIFY_QUIET_DEFAULT_START: &str = "22:00";

/// 조용한 시간의 기본 끝(꺼져 있을 때 보이는 값).
pub const NOTIFY_QUIET_DEFAULT_END: &str = "07:00";

/// 바꾸기 거부: 잠긴 종류의 두 전달처를 모두 끄려 했다(400).
pub const NOTIFY_LOCKED_OFF: &str = "locked_kind_off";

/// 바꾸기 거부: 조용한 시간의 시각 글자가 틀렸다(400).
pub const NOTIFY_INVALID_QUIET: &str = "invalid_quiet_hours";

mod book;
mod http;

pub(crate) use book::{NotifyBook, NotifySettings};
pub(crate) use http::routes;
