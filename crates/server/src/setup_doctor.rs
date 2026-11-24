//! (31차 1번 (라)) 설정 화면의 진단 읽기([`SETUP_DOCTOR_PATH`], 계약 C32 · V78 · R18-4).
//!
//! `price-inha doctor`(CLI)는 보관 폴더의 저장소를 따로 열고, 주소를 열어 보고, 서비스 명령을 부르고, 뜻 검색이
//! 켜져 있으면 Ollama에 묻는다. 실행 중인 서버에서 그것을 돌리면 열린 저장소 · 포트와 부딪히고 바깥에 요청이
//! 나간다. 그래서 이 경로는 **실행 중인 서버가 이미 아는 것만** 모은 읽기 전용 요약이다: 파일 · 저장소를 새로
//! 열지 않고, 아무것도 고치지 않으며, 바깥(증권사 · AI · 자료 · 네이버 · Ollama · GitHub)으로 요청을 만들지 않는다.
//!
//! # 진단 (`GET` [`SETUP_DOCTOR_PATH`], 세션, 200)
//!
//! ```json
//! {"items": [
//!   {"key": "store",          "label": "저장소",          "status": "ok",   "detail": "저장소 판 11",              "todo": null},
//!   {"key": "keys",           "label": "키 등록",         "status": "ok",   "detail": "증권사 있음 · AI 키 있음 · …", "todo": null},
//!   {"key": "ai",             "label": "AI 연결",         "status": "info", "detail": "API 키",                    "todo": null},
//!   {"key": "ai_daily_limit", "label": "하루 AI 호출 상한", "status": "info", "detail": "없음",                      "todo": null},
//!   {"key": "backup",         "label": "백업",            "status": "info", "detail": "백업이 꺼져 있습니다",        "todo": null},
//!   {"key": "demo",           "label": "시연 모드",        "status": "info", "detail": "시연 모드 …",               "todo": null},
//!   {"key": "market",         "label": "시장",            "status": "info", "detail": "국내 (kr)",                 "todo": null}
//! ]}
//! ```
//!
//! - 맨 위 칸은 `items` 하나뿐이다. 항목은 [`DIAGNOSIS_ITEMS`]의 순서이고 칸은 `key` · `label` · `status` ·
//!   `detail` · `todo` 다섯뿐이다. 수는 싣지 않는다(판 번호 · 상한 · 시각도 `detail` 글자 안에 둔다).
//! - `status`: [`DIAG_OK`] · [`DIAG_WARN`] · [`DIAG_FAIL`] · [`DIAG_INFO`]. `todo`(사람이 할 일 글)는
//!   `warn` · `fail`일 때만 글자이고 `ok` · `info`이면 `null`이다.
//! - `store`: 열려 있는 저장소의 판(`pi_store::SCHEMA_VERSION`) — `ok`.
//! - `keys`: 칸 이름과 "있음" · "없음"뿐이다(키 값 · 일부 · 길이 없음, V46 · N8). 필수(증권사 · 고른 AI 연결)가
//!   갖춰졌으면 `ok`, 아니면 시연 모드에서는 `warn` · 실제 모드에서는 `fail`이고 `todo`는 키 등록 안내다.
//! - `ai`: 고른 AI 연결 방식("API 키" · "Claude Code CLI") — `info`.
//! - `ai_daily_limit`: 하루 AI 호출 상한("없음" · "N번") — `info`.
//! - `backup`: 꺼짐 → `info`, 켜짐 · 마지막 백업 시각(한국 시각) → `ok`, 켜짐 · 아직 없음 → `info`,
//!   지난 백업이 실패해 다시 올리기를 기다림 → `warn`과 할 일.
//! - `demo`: 시연 모드인가 — `info`. 시연이면 `detail`에 "시연"이 들어 있다.
//! - `market`: 켤 때 정한 시장 — `info`, `detail`은 "국내 (kr)" · "미국 (us)".
//!
//! 원격(다른 기기)은 세션이 없으면 401이다(다른 설정 읽기와 같다). 세션이 있으면 원격에서도 읽는다.

/// 진단을 읽는 경로(`GET`, 읽기만). 계약 C32의 이름을 따른다.
pub const SETUP_DOCTOR_PATH: &str = "/api/setup/doctor";

/// 진단 항목의 `key`. 이 순서대로 낸다.
pub const DIAGNOSIS_ITEMS: [&str; 7] = [
    "store",
    "keys",
    "ai",
    "ai_daily_limit",
    "backup",
    "demo",
    "market",
];

/// 진단 상태: 문제없음.
pub const DIAG_OK: &str = "ok";
/// 진단 상태: 살펴볼 것(할 일이 있다).
pub const DIAG_WARN: &str = "warn";
/// 진단 상태: 문제(할 일이 있다).
pub const DIAG_FAIL: &str = "fail";
/// 진단 상태: 알림(좋고 나쁨이 없는 값).
pub const DIAG_INFO: &str = "info";
