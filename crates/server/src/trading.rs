//! (W5 1차) 규칙 · 주문 · 템플릿 · 멈춤 · 승인 잇기: T6 · T7 · T8 · T13 · T14 연결.
//!
//! 작업 지시서: `docs/dev/modules/server.md` "1차".
//!
//! - 주문 실행기(`pi_exec::Executor::open`)가 돌려주는 점검 권한(`pi_paper::CheckAuthority`)은
//!   **규칙 엔진의 길에만** 건넨다(`pi_rules::issue`). 다른 어디에도 보관하거나 내주지 않는다.
//! - 주문 안 하나([`OrderRequest`])는 팀 상태 → 쉬는 날(E19) → 규칙 점검(`pi_rules::check`) 순으로 본다.
//!   지나면 점검 통과 표시를 받아 실행기로 내고, 금액 한도를 넘으면 승인 요청(C12)을 만든다.
//! - 승인 증표(`pi_rules::ApprovalGrant`)는 **승인 모듈이 소비에 성공한 뒤에만** 여기서 만든다
//!   (`pi_approval::Approvals::consume` → 증표 → `pi_rules::approved` → 실행). 화면이나 다른 모듈이
//!   준 번호 · 지문 글자로 증표를 만들지 않는다(exec 1차 검토의 참고).
//! - 손실 · 멈춤 판단(`pi_rules::evaluate_stops`)이 요구한 전이와 주문 실행기의 이상 감지 멈춤 요구
//!   (`pi_exec::Error::AnomalyStop`)는 저장소의 팀 상태 전이(`pi_store::Store::change_team_state`)로 한다.
//! - 시각은 [`crate::Wiring::clock`]에서만 읽는다. 돈은 십진수다.

mod account;
mod answer;
mod base;
#[cfg(test)]
mod basis_order;
#[cfg(test)]
mod books_race;
mod chosen;
mod day;
mod due;
mod http;
mod link;
mod marks;
#[cfg(test)]
mod money_safety;
mod order;
mod promote;
mod retry;
mod rules_config;
mod rules_config_http;
mod stop;
mod templates;
mod view;
mod watch;
// (50차) 승인 카드 보강: 투표 수 · 한도 적용 전 → 후.
mod shown;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::NaiveDate;
use pi_approval::{ApprovalNumber, Approvals, Fingerprint};
use pi_exec::Executor;
use pi_gate::Publisher;
use pi_market::{Clock, Market, MarketCalendar, Quote};
use pi_paper::{CheckAuthority, OrderId, PaperBroker, PaperConfig};
use pi_rules::{Blocked, OrderDraft, RulesConfig, SymbolStatus};
use pi_store::{Store, TeamState, TradeId};
use rust_decimal::Decimal;

use crate::TradingError;
use crate::desk::Desk;
use answer::Waiting;
use base::{Calendars, DayLog};
use marks::Marks;
use watch::Watch;

pub(crate) use answer::{Answered, reject_word, rejected};
pub(crate) use base::{TradingParts, default_wiring};
pub(crate) use chosen::CHOOSE_TEMPLATE_NOTICE;
pub(crate) use http::routes;
pub(crate) use link::{
    AnsweredHook, ApprovalHook, CloseHook, DataHaltHook, Hooks, PROPOSAL_UNAVAILABLE, ProposalHook,
    StuckStopHook, approval_agent, approval_state_word, approval_status_word,
};
pub(crate) use rules_config::{demo_sectors, load as load_rules_config};
pub(crate) use templates::change_of;
pub(crate) use view::change_json;

/// 투자 팀 상태 읽기(세션이 있어야 열린다). `GET` →
/// `{"state": 팀 상태 글자, "plan": "trade" | "rest" | null, "reason": 쉬는 이유 | null}`.
/// (40차) 전체 정지면 `"stop_cause"`에 그 정지의 사유(`"owner_stop"` · `"max_loss"`), 아니면 `null`.
///
/// `plan` · `reason`은 **오늘**(시장 달력의 날짜) 팀장이 정한 것이다([`Trading::decide_day`]).
/// 오늘 정한 것이 없으면 둘 다 `null`이다.
pub const TEAM_PATH: &str = "/api/team";

/// 다시 시작(C8). `POST`, 몸통 없음. 세션과 같은 출처가 있어야 한다.
///
/// - 팀이 전체 정지 · 이상 감지 멈춤이고 필수 키(증권사 · AI)가 모두 등록돼 있으면 쉬는 중으로 바꾼다
///   (사유 `TeamCause::OwnerRestart`, `owner_approved` 참). 응답 200 `{"state": "resting"}`.
/// - 필수 키가 하나라도 없으면 409 `{"error": `[`RESTART_KEYS_REQUIRED`]`, "message": 안내}`(E9).
///   안내는 키를 먼저 등록하라는 말이다. 팀은 그대로 멈춰 있고 기록도 더하지 않는다.
/// - 팀이 멈춰 있지 않으면(운용 중 · 쉬는 중) 409 `{"error": `[`RESTART_NOT_STOPPED`]`}`.
///   하루 한도 멈춤은 다음 거래일 시작에만 풀리므로 이것도 409 [`RESTART_NOT_STOPPED`]다.
pub const TEAM_RESTART_PATH: &str = "/api/team/restart";

/// (40차 1번 · 결정 25 · F4 · R14-1) 사람이 내리는 전체 정지. `POST`, 몸통 없음 또는 `{}`. 세션과 같은 출처가 있어야
/// 한다(세션이 없으면 401). **원격 세션으로도 된다** — 정지는 위험을 줄이는 쪽이라 이 컴퓨터로 막지 않는다
/// (다시 시작 [`TEAM_RESTART_PATH`]의 규칙은 그대로다).
///
/// - 팀이 전체 정지가 아니면(운용 중 · 쉬는 중 · 하루 한도 멈춤 · 이상 감지 멈춤) 전체 정지로 바꾼다
///   (사유 `TeamCause::OwnerStop`, `owner_approved` 참, 기록의 까닭 글 "내가 정지"). 응답은 다시 시작과 같은 모양:
///   200 `{"state": "full_stop"}`(바뀐 팀 상태 글자, 쓰는 템플릿 안내가 있으면 `"notice"`도).
/// - 이미 전체 정지면 409 `{"error": `[`STOP_ALREADY_STOPPED`]`}`이고 기록을 더하지 않는다.
/// - 정지한 뒤는 최대 손실 전체 정지와 같다: 새 주문은 막히고 보유의 손절 · 파는 주문은 계속 돌며(F4), 실전중
///   템플릿은 멈춤이 된다(저장소). 최대 손실 전체 정지 때 나가는 이벤트 · 알림이 그대로 나간다(까닭 글만 "내가 정지").
/// - 다시 시작은 사람이 계좌 화면에서만 한다(결정 20). 담당(AI) 도구에는 이 길이 없다.
/// - 메신저 `/stop`(허용된 사람)도 이 길과 같은 함수를 탄다(`crate::messenger` 문서).
pub const TEAM_STOP_PATH: &str = "/api/team/stop";

/// 승인 답하기(C7). `POST` `{"number": 승인 번호, "fingerprint": 화면 지문(64자), "answer": "approve" | 그 밖}`.
/// 세션과 같은 출처가 있어야 한다. 응답은 언제나 200 `{"result": "approved" | "rejected", "reason": 글자 | null}`.
///
/// - 승인이면 그 자리에서 승인 소비 → 다시 점검 → 실행까지 한다. 실행했으면 `result`는 `"approved"`이고
///   매매는 주문됨이다.
/// - 승인했지만 다시 점검한 주문 내용이 승인한 것과 다르면(시세가 바뀌어 값 · 수량이 달라짐 등) 내지 않는다:
///   `"rejected"`, 이유 [`ANSWER_CONTENT_CHANGED`](E7). 매매는 막힘(승인 거절)이다.
/// - 지문이 다르면 `"rejected"`, 이유 [`ANSWER_FINGERPRINT_MISMATCH`](E7). 매매는 막힘(승인 거절)이다.
/// - 없는 번호는 `"rejected"`, 이유 [`ANSWER_NOT_FOUND`](E8). 아무것도 바뀌지 않는다.
/// - 거절이면 `"rejected"`, 이유 `"user_rejected"`. 매매는 막힘(승인 거절)이다.
/// - 이미 처리된 번호는 `"rejected"`, 이유 `"already_resolved"`. 첫 결과는 그대로이고 내지 않는다.
/// - (1차 보강) 승인했지만 그 사이 오늘을 쉬는 날로 정했으면 사는 주문 · 익절은 내지 않는다(E19):
///   `"rejected"`, 이유 [`ANSWER_REST_DAY`]. 매매는 막힘(승인 거절)이다. 손절 · 청산은 그대로 낸다.
pub const APPROVAL_ANSWER_PATH: &str = "/api/approvals/answer";

/// 템플릿 고르기(C4). `POST` `{"style": "day_trade" | "swing" | "long_term"}` → 200
/// `{"id": 템플릿 번호, "style": 스타일, "state": "simulating"}`. 그 스타일의 기본값으로 저장소에 새 템플릿을
/// 만들고(모의중), 그것을 지금 쓰는 템플릿으로 한다(가장 나중에 고른 모의중 템플릿을 쓴다).
pub const TEMPLATE_PATH: &str = "/api/templates";

/// 템플릿 값 맞추기(C14). `POST` `{"id": 템플릿 번호, "changes": {칸: 십진수 글자}}` → 200
/// `{"id", "revision": 새 판 번호, "state": "simulating"}`.
///
/// 받는 칸: `risk_per_trade_pct` · `daily_loss_limit_pct` · `max_drawdown_pct` · `symbol_weight_pct` ·
/// `sector_weight_pct` · `min_cash_pct` · `auto_order_limit_pct`(퍼센트 단위). 모르는 칸 · 읽을 수 없는 값은
/// 400 `{"error": "invalid_input"}`. 절대 상한을 넘거나 아래쪽 경계를 어기면(`pi_template::adjust`의 거부)
/// 400 `{"error": `[`TEMPLATE_OVER_CAP`]`, "fields": [어긴 칸의 받는 이름(위 목록의 글자)]}`(E5)이고 새 판을 만들지 않는다.
///
/// (21차 1번 · 27차 3번 나) 실전중 템플릿의 위험을 키우는 바꾸기는 200
/// `{"state": "needs_approval", "proposal": 개선안 번호, "widened": [키운 칸], "number": 승인 번호 글자}`다.
/// `number`는 그 개선안의 승인 요청 번호다(화면이 "승인 뒤 다음 거래일부터"와 함께 보인다).
pub const TEMPLATE_ADJUST_PATH: &str = "/api/templates/adjust";

/// (27차 1번 · C28) 템플릿 화면 읽기. `GET`, 세션이 있어야 한다(원격은 세션 없이 401) →
/// 200 `{"current": 쓰는 템플릿 번호 | null, "templates": [템플릿, …]}`. 템플릿이 없으면 `{"current": null, "templates": []}`.
///
/// `current`는 쓰는 템플릿(가장 나중에 고른 것, 상태를 가리지 않는다)이다. `templates`는 번호 순서(작은 것부터)이고
/// 템플릿 하나는:
///
/// ```text
/// {
///   "id": 번호,
///   "style": "day_trade" | "swing" | "long_term",
///   "state": "simulating" | "promotion_pending" | "live" | "stopped",
///   "revision": 지금 판 번호,
///   "fields": [{"key", "value", "cap": 십진수 글자 | null, "editable": 참 | 거짓}],
///   "revisions": [{"revision", "at": 유닉스 밀리초, "changes": [{"key", "before", "after", "widened"}],
///                  "proposal": 개선안 번호 | null}],
///   "promotion": [{"criterion": "trades" | "trading_days" | "net_pnl" | "max_drawdown" | "out_of_sample",
///                  "need": 십진수 글자, "have": 십진수 글자 | null, "met": 참 | 거짓}],
///   "approval": 기다리는 실전 승격 승인 번호 글자 | null
/// }
/// ```
///
/// - `fields`는 `pi_template::template_view`의 칸(순서 · 키 · 값 · 상한)이고, `editable`은 모의중 · 실전중일 때 참이다
///   (승격대기 · 멈춤은 잠겨 모두 거짓). 실전중에 값을 저장하면 서버의 지금 길을 탄다: 위험을 키우는 변경은 승인 대기,
///   그 밖은 저장소 잠금 거절(409).
/// - `revisions`는 `pi_store::Store::template_history`의 판 순서다. `changes`는 앞 판과의 차이
///   (`pi_template::diff_params`, 첫 판은 빈 목록)이고 `proposal`은 사용자 변경 개선안으로 생긴 판의 개선안 번호다.
/// - `promotion`은 `pi_template::promotion_lines`(그 템플릿의 승격 기준 · 그 템플릿을 만든 뒤의 모의 결과)다.
/// - `approval`은 승격대기일 때 기다리는 실전 승격 승인 요청의 번호다(화면의 "실전 운용 시작"이 지금 승인 카드
///   길 [`crate::APPROVAL_PATH`] · [`crate::APPROVAL_DECIDE_PATH`]로 잇는다). 그 밖의 상태는 `null`.
pub const TEMPLATES_VIEW_PATH: &str = "/api/templates/current";

/// (27차 2번 · F6-9a · E58) 실전 템플릿 멈추기. `POST /api/templates/{id}/stop`, 몸통 없음. 세션과 같은 출처가 있어야
/// 한다. 승인을 거치지 않고 바로 한다.
///
/// - 실전중이면 멈춤으로 옮긴다(사용자 멈춤 전이, `pi_store::Store::stop_template`): 200 `{"id", "state": "stopped"}`.
/// - 실전중이 아니면(모의중 · 승격대기 · 멈춤) 409 `{"error": `[`TEMPLATE_NOT_LIVE`]`, "state": 지금 상태 글자}`이고 그대로다.
/// - 없는 번호는 404 `{"error": "not_found"}`, 번호로 읽을 수 없으면 400 `{"error": "invalid_input"}`.
/// - 멈춘 뒤 다시 실전으로 가는 길은 없다(새로 고른다).
pub const TEMPLATE_STOP_PATH: &str = "/api/templates/{id}/stop";

/// (30차 1번 · C31 · V86 · R3-1 · R16-5) 모의계좌 읽기. `GET`, 세션이 있어야 한다(원격은 세션 없이 401).
/// 팀을 잇지 않고 띄운 서버에서도 읽힌다(주문 길의 경로다). 응답 200:
///
/// ```text
/// {
///   "account": "paper",
///   "cash": [{"currency": "KRW" | "USD", "available", "held", "unsettled"}],
///   "holdings": [{"symbol", "market": "kr" | "us", "currency", "quantity", "average_price",
///                 "last_price": 십진수 글자 | null, "value": 십진수 글자 | null, "pnl": 십진수 글자 | null}],
///   "equity": [{"currency", "total", "day_start", "day_pnl", "peak", "drawdown_pct",
///               "drawdown_limit_pct": 십진수 글자 | null}]
/// }
/// ```
///
/// - 금액 · 수량 · 퍼센트는 모두 **십진수 글자**다(부동소수점 수를 쓰지 않는다). 모르는 값은 `null`이다.
/// - `account`는 언제나 [`ACCOUNT_PAPER`]다: 계좌는 모의계좌(pi-paper)뿐이다(AGENTS 6번 · 결정 17).
/// - `cash`는 통화(KRW, USD 차례)마다 한 줄 — `pi_paper::PaperBroker::account_snapshot`의 쓸 수 있는 · 묶인 ·
///   결제 전 돈이다.
/// - `holdings`는 보유 종목(수량이 0인 것은 없다). `last_price`는 그 종목의 마지막으로 받은 시세이고, 받은 시세가
///   없으면 `null`이며 그때 `value`(= 수량 × 지금 값) · `pnl`(= `value` − 수량 × 평균 단가)도 `null`이다.
/// - `equity`는 통화마다 한 줄 — 주문 길이 하루 손실 · 낙폭 판정에 쓰는 장부(`trading/marks.rs`)와 같은 값이다:
///   `total` 지금 평가액(현금 전부 + 보유 평가, 시세가 없는 종목은 평균 단가로), `day_start` 그날 시작 평가액,
///   `day_pnl` = `total` − `day_start`(오늘 손익, 평가 손익 포함), `peak` 최고 평가액(처음 돈 아래로 내려가지 않는다),
///   `drawdown_pct` = (`peak` − `total`) ÷ `peak` × 100(소수 둘째 자리 반올림, 최고 위면 0),
///   `drawdown_limit_pct` 지금 쓰는 템플릿의 `max_drawdown_pct`(쓰는 템플릿이 없으면 `null`).
pub const ACCOUNT_PATH: &str = "/api/account";

/// 계좌 읽기 응답의 `account` 값: 모의계좌.
pub const ACCOUNT_PAPER: &str = "paper";

/// 템플릿 멈추기 거부: 실전중이 아니다.
pub const TEMPLATE_NOT_LIVE: &str = "not_live";

/// 다시 시작 거부: 필수 키를 먼저 다시 등록해야 한다(E9).
pub const RESTART_KEYS_REQUIRED: &str = "keys_required";
/// 다시 시작 거부: 팀이 다시 시작할 멈춤 상태가 아니다.
pub const RESTART_NOT_STOPPED: &str = "not_stopped";
/// (40차 1번) 전체 정지 거부: 이미 전체 정지다([`TEAM_STOP_PATH`]).
pub const STOP_ALREADY_STOPPED: &str = "already_stopped";
/// 승인 답 결과의 이유: 승인한 뒤 실행할 내용이 바뀌었다(E7).
pub const ANSWER_CONTENT_CHANGED: &str = "content_changed";
/// 승인 답 결과의 이유: 화면 지문이 요청의 지문과 다르다(E7).
pub const ANSWER_FINGERPRINT_MISMATCH: &str = "fingerprint_mismatch";
/// 승인 답 결과의 이유: 없는 승인 번호다(E8).
pub const ANSWER_NOT_FOUND: &str = "not_found";
/// 승인 답 결과의 이유: 승인한 사이 오늘을 쉬는 날로 정했다(E19).
pub const ANSWER_REST_DAY: &str = "rest_day";
/// 템플릿 값 거부: 절대 상한 · 아래쪽 경계(E5).
pub const TEMPLATE_OVER_CAP: &str = "over_cap";

/// 팀 상태 글자: 운용 중.
pub const TEAM_OPERATING: &str = "operating";
/// 팀 상태 글자: 쉬는 중.
pub const TEAM_RESTING: &str = "resting";
/// 팀 상태 글자: 하루 한도 멈춤.
pub const TEAM_DAILY_LIMIT_HALT: &str = "daily_limit_halt";
/// 팀 상태 글자: 이상 감지 멈춤.
pub const TEAM_ANOMALY_HALT: &str = "anomaly_halt";
/// 팀 상태 글자: 전체 정지.
pub const TEAM_FULL_STOP: &str = "full_stop";

/// 저장되는 이벤트 종류: 하루 시작(V5). 내용 `{"date": "YYYY-MM-DD"}`(시장 달력의 날짜).
///
/// 시계가 그 날 첫 세션 시작에 닿으면(또는 띄웠을 때 이미 그 날 세션 안이면) **그 날에 한 번만** 남긴다.
/// 이때 하루 한도 멈춤을 쉬는 중으로 풀고(`TeamCause::NextTradingDayStart`), 그날 시작 평가액을 새로 잡는다.
/// 아무도 부르지 않아도 뒤에서 돈다.
pub const KIND_DAY_START: &str = "day_start";

/// 저장되는 이벤트 종류: 하루 마감(V5). 내용 `{"date": "YYYY-MM-DD"}`.
///
/// 시계가 그 날 마지막 세션 끝에 닿으면 그 날에 한 번만 남긴다. 운용 중이었으면 쉬는 중으로 바꾼다
/// (`TeamCause::MarketClose`). 아무도 부르지 않아도 뒤에서 돈다.
pub const KIND_DAY_CLOSE: &str = "day_close";

/// 저장되는 이벤트 종류: 팀장의 오늘 판단(V7). 내용
/// `{"date": "YYYY-MM-DD", "plan": "trade" | "rest", "reason": 쉬는 이유 | null}`.
pub const KIND_DAY_PLAN: &str = "day_plan";

/// (35차 1번, 결정 19번 · P5 · P8) 보관 폴더의 금지 종목 · 업종표 파일 이름. 모양:
///
/// ```text
/// {"banned": [{"market": "kr" | "us", "symbol": 종목 글자}], "sectors": {"<종목 글자>": "업종 이름", …}}
/// ```
///
/// - 켤 때(`App::launch_with` — 따라서 `App::start` · `App::launch_run`도, 시연 모드여도) 읽어 주문 길의
///   [`TradingWiring::rules`]를 **대신한다**(파일 값이 주문 전 점검의 금지 종목(P5) · 업종표(P8)가 된다).
/// - 파일이 없으면 조립이 준 값 그대로다(기본 조립은 빈 표 — 지금과 같이 표에 없는 종목의 사는 주문은 P8로 막는다).
/// - 파일이 깨졌으면(JSON이 아님 · 모양이 다름 · 값 검사([`RULES_CONFIG_PATH`]의 `POST`와 같은 규칙)에 걸림)
///   켜지지 않고 `AppError::BadFile { file: RULES_CONFIG_FILE, .. }`로 까닭을 말한다(조용히 빈 표로 돌지 않는다).
/// - 시연 조립(`crate::run_assembly`, `--demo`)의 시연 업종("시연 업종 A" 식)은 조립 값으로만 넣고 이 파일에 쓰지
///   않는다(E56). 파일이 있어도 지켜보는 종목 가운데 업종이 없는 것에는 시연 업종이 더해진다. `App::launch_with`에
///   직접 준 조립 값에는 (시연 설정이어도) 시연 업종을 더하지 않는다(기존 시험의 빈 표 조립이 그대로다).
pub const RULES_CONFIG_FILE: &str = "rules-config.json";

/// (35차 2번, R7-3 · F5) 금지 종목 · 업종표 읽기 · 고치기.
///
/// `GET` → 200
///
/// ```text
/// {"banned": [{"market": "kr" | "us", "symbol"}],          // 저장된 차례
///  "sectors": [{"symbol", "sector"}],                         // 종목 글자 차례
///  "editable": bool,                                          // 이 요청이 그 컴퓨터에서 왔는가(원격이면 false)
///  "unsectored": [종목 글자]}                                 // 지켜보는 종목 가운데 업종표에 없는 것(지켜보는 차례)
/// ```
///
/// `POST` `{"banned": [...], "sectors": [{"symbol", "sector"}]}`(같은 모양, 두 칸뿐)로 **통째** 바꾼다.
/// - 세션이 있어야 한다(없으면 관문이 401 — `GET`도). 고치기는 **그 컴퓨터에서만**: 원격 세션의 `POST`는 403이고
///   저장하지 않으며 바꾼 일도 남기지 않는다(하루 AI 상한과 같은 관문 — 규칙을 푸는 일은 사용자가 직접). 원격 세션의
///   `GET`은 200이고 `"editable": false`다.
/// - 값 검사: 종목은 `pi_market::Symbol::parse` 글자, 시장은 `"kr"` · `"us"`, 업종 이름은 앞뒤 공백을 뗀 뒤
///   1자 이상 [`RULES_SECTOR_MAX_CHARS`]자 이하, 같은 종목이 업종표에 두 번 · 같은 (시장, 종목)이 금지 목록에
///   두 번이면 안 된다. 걸리면 400 `{"error": "invalid_input"}`이고 아무것도 저장하지 않는다(파일 · 점검 값 그대로).
/// - 저장은 원자적으로(임시 파일 → 옮기기) [`RULES_CONFIG_FILE`]에 쓰고, 바꾼 값은 **다시 켜지 않아도** 다음
///   주문 전 점검부터 쓰인다. 응답은 200과 `GET`과 같은 모양(바꾼 뒤 값).
/// - 바꾼 일을 [`KIND_RULES_CONFIG`] 이벤트로 남긴다.
pub const RULES_CONFIG_PATH: &str = "/api/rules/config";

/// (35차 2번) 업종 이름의 가장 긴 글자 수(앞뒤 공백을 뗀 뒤 센다).
pub const RULES_SECTOR_MAX_CHARS: usize = 40;

/// (35차 2번) 저장되는 이벤트 종류: 금지 종목 · 업종표를 바꿨다. 내용
/// `{"at": 유닉스 밀리초, "by": "local", "banned": 금지 종목 수, "sectors": 업종표 줄 수}`(값 자체는 싣지 않아도 된다).
pub const KIND_RULES_CONFIG: &str = "rules_config";

/// 날짜 → 그 시장의 달력. 조립하는 쪽(테스트)이 넣는다.
pub type CalendarFn = Arc<dyn Fn(NaiveDate) -> MarketCalendar + Send + Sync>;

/// 주문 길의 바깥 값. [`crate::App::start_with`]에 준다.
#[derive(Clone)]
pub struct TradingWiring {
    /// 모의계좌 설정(처음 돈 · 시장 규칙). 저장소에 계좌 스냅샷이 있으면 처음 돈은 쓰지 않는다(V6).
    pub paper: PaperConfig,
    /// 금지 종목 · 업종표(결정 19번).
    pub rules: RulesConfig,
    /// 날짜(시장 시간대의 오늘) → 달력. `Some`이면 주문 길 · 모의계좌 · 하루 시작/마감은 이것만 쓴다
    /// (시세 어댑터의 달력을 쓰지 않는다). `None`이면 시세 어댑터의 달력을 쓴다.
    pub calendar: Option<CalendarFn>,
}

/// 팀장의 오늘 판단(2차에서는 팀장 에이전트가 부른다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DayPlan {
    /// 오늘 매매한다: 쉬는 중이면 운용 중으로 바꾼다(`TeamCause::TradingStart`).
    Trade,
    /// 오늘 쉰다(E19 · V7): 운용 중이면 쉬는 중으로 바꾼다. 이유는 [`TEAM_PATH`]에 보인다.
    Rest {
        /// 쉬는 이유(사람이 읽는 말).
        reason: String,
    },
}

/// 주문 안 하나(C9 입력과 그 종목의 바깥 값).
///
/// 매매(`trade_id`)는 저장소에 있어야 한다. 매매의 상태는 저장소의 것을 쓰고 `draft.stage`는 믿지 않는다.
#[derive(Debug, Clone)]
pub struct OrderRequest {
    /// 매매 번호.
    pub trade_id: TradeId,
    /// 주문 안.
    pub draft: OrderDraft,
    /// 그 종목의 오늘 기준가(전일 종가). 가격제한폭과 모의계좌에 쓴다.
    pub reference_price: Decimal,
    /// VI · 거래정지 표시(모르면 사는 주문을 막는다).
    pub status: SymbolStatus,
}

/// 주문 안을 처리한 결과.
#[derive(Debug, Clone, PartialEq)]
pub enum Placed {
    /// 점검을 지나 모의계좌에 냈다. 매매는 주문됨이다.
    Ordered(OrderId),
    /// 금액 한도를 넘어 승인을 기다린다(C12). 매매는 승인대기다. 승인 요청은 저장된 이벤트로도 나간다.
    AwaitingApproval {
        /// 승인 번호.
        number: ApprovalNumber,
        /// 승인할 내용의 지문.
        fingerprint: Fingerprint,
    },
    /// 규칙 엔진이 막았다(이유가 기록과 화면에 남는다, V26). 내지 않았다.
    Blocked(Blocked),
    /// 오늘은 쉬는 날이다(E19): 사는 주문과 익절은 내지 않는다(손절 · 청산만 한다). 쉬는 이유를 담는다.
    RestDay(String),
}

/// 주문 길 하나(앱마다 하나). [`crate::App::trading`]으로 받는다.
pub struct Trading {
    store: Store,
    clock: Arc<dyn Clock>,
    market: Market,
    desk: Arc<Desk>,
    executor: Executor,
    /// 주문 실행기가 준 점검 권한. 규칙 엔진의 길(`pi_rules::issue`)에만 건넨다.
    authority: CheckAuthority,
    approvals: Approvals,
    paper_config: PaperConfig,
    rules: rules_config::RulesHold,
    calendars: Calendars,
    /// 모의계좌에 달력을 넣은 날짜.
    paper_day: Mutex<Option<NaiveDate>>,
    /// 종목마다 마지막으로 받은 시세.
    quotes: Mutex<HashMap<String, Quote>>,
    /// 통화별 그날 시작 · 최고 평가액과 마지막으로 셈한 장부.
    marks: Mutex<Marks>,
    /// (14차) 손실 기준을 셈 · 꺼내기 · 저장하는 순서를 한 줄로 세운다(`marks`보다 먼저 잡는다).
    save_turn: Mutex<()>,
    /// 하루 시작 · 마감과 오늘 판단.
    days: Mutex<DayLog>,
    /// 승인을 기다리는 주문 안(승인 번호 → 다시 점검할 것).
    waiting: Mutex<HashMap<String, Waiting>>,
    /// 손절로 낸 파는 주문(매매 번호 → 주문 번호). 같은 매매의 손절을 두 번 내지 않는다(E30).
    stops: Mutex<HashMap<TradeId, OrderId>>,
    /// 팀 상태를 읽고 바꾸는 일을 한 줄로 세운다.
    team_turn: Mutex<()>,
    /// (10차 10a) 종목별 최근 시세 · 기준가 · 이상 감지의 기억.
    watch: Mutex<Watch>,
    /// (10차 10a) 답 없는 승인 요청의 처리를 한 줄로 세운다.
    due_turn: tokio::sync::Mutex<()>,
    /// (10b 보강 5번) 막힌 다시 내기의 물러서기(매매 번호 → 다음에 볼 때 · 까닭). 기다리는 대기만큼만 둔다.
    held_back: Mutex<HashMap<TradeId, retry::HeldBack>>,
    /// 저장되는 이벤트를 내보내는 손잡이(접속 관문이 뜬 뒤에 붙는다).
    publisher: OnceLock<Publisher>,
    /// (W5 3차) 승인 요청 · 하루 마감을 알리는 자리(3차 조립이 붙인다).
    hooks: OnceLock<Hooks>,
    /// (28차 3번) 승인 카드 길로 온 개선안 답을 개선안 답하기 길로 보내는 자리(팀을 연 조립이 붙인다).
    proposal_hook: OnceLock<ProposalHook>,
}

impl Trading {
    /// 모의계좌. 조립하는 쪽(테스트)이 장애 흉내를 켜거나 주문 · 보유를 읽는다.
    pub fn paper(&self) -> &PaperBroker {
        self.executor.broker()
    }

    /// 팀장의 오늘 판단을 받는다. 바뀐 뒤의 팀 상태를 돌려준다. [`KIND_DAY_PLAN`]을 남긴다.
    ///
    /// - 필수 설정(증권사 · AI 키, 종목)이 빠져 있고 시연 모드가 아니면 [`TradingError::SetupRequired`](V31).
    /// - [`DayPlan::Trade`]인데 팀이 하루 한도 멈춤 · 이상 감지 멈춤 · 전체 정지면
    ///   [`TradingError::Halted`]이고 상태는 그대로다(V9: 전체 정지는 내 승인으로만 풀린다).
    /// - 이미 그 상태면 전이 없이 그 상태를 돌려준다.
    pub fn decide_day(&self, plan: DayPlan) -> Result<TeamState, TradingError> {
        self.decide(plan)
    }

    /// 주문 안 하나를 점검하고 낸다(C9 → C10, 승인이 필요하면 C12).
    ///
    /// 순서: 필수 설정 → 지금 쓰는 템플릿 → 오늘 쉬는 날이면 사는 주문 · 익절은 [`Placed::RestDay`](E19) →
    /// `pi_rules::check`(팀 상태는 저장소의 것, 장부는 모의계좌의 것, 시세는 [`Trading::on_quote`]로 받은
    /// 그 종목의 마지막 시세) → 통과면 `pi_rules::issue` → `pi_exec::Executor::submit`/`sell`.
    ///
    /// - 필수 설정이 빠졌으면 [`TradingError::SetupRequired`], 고른 템플릿이 없으면 [`TradingError::NoTemplate`].
    /// - 실행기가 이상 감지 멈춤을 요구하면(증권사가 받지 않음 · 응답 없음) 팀을 이상 감지 멈춤으로 바꾸고
    ///   (`TeamCause::AnomalyDetected`) [`TradingError::Broker`]를 돌려준다.
    pub async fn place(&self, request: OrderRequest) -> Result<Placed, TradingError> {
        self.place_order(request).await
    }

    /// 시세 하나를 받는다(시세 흐름도 부른다). 모의계좌 체결 → 매매 상태(주문됨 → 보유중 등) →
    /// 장부 → 손실 · 멈춤 판단까지 마친 뒤 돌아온다. `pi_rules::evaluate_stops`가 요구한 전이를
    /// 저장소에 한다(하루 한도 멈춤: `TeamCause::DailyLossLimit`, 전체 정지: `TeamCause::MaxLoss`).
    pub async fn on_quote(&self, quote: Quote) -> Result<(), TradingError> {
        self.take_quote(quote).await
    }

    /// (10차 10a) 이 주문 길의 시계의 지금 시각.
    pub(crate) fn now(&self) -> chrono::DateTime<chrono::Utc> {
        self.clock.now()
    }

    /// (W5 2차) 이 주문 길의 시장.
    pub(crate) fn market(&self) -> Market {
        self.market
    }

    /// (W5 2차) 그 날짜의 달력(조립하는 쪽이 준 것, 아니면 시세 흐름이 받은 그 날짜의 것). 모르면 없음.
    pub(crate) fn calendar_on(&self, date: NaiveDate) -> Option<MarketCalendar> {
        match &self.calendars {
            Calendars::Fixed(calendar) => Some(calendar(date)),
            Calendars::Feed(held) => base::lock(held).clone().filter(|c| c.today.date == date),
        }
    }

    /// (W5 2차) 오늘(시장 달력의 날짜) 이미 정한 하루 판단.
    pub(crate) fn plan_today(&self) -> Option<DayPlan> {
        let today = self.date_of(self.clock.now());
        let days = base::lock(&self.days);
        days.plan
            .as_ref()
            .filter(|(date, _)| *date == today)
            .map(|(_, plan)| plan.clone())
    }
}
