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

use std::sync::Arc;

use chrono::{FixedOffset, TimeZone};
use pi_agent::Role;
use pi_market::Market;
use pi_secrets::KeySlot;
use pi_store::Store;
use serde_json::{Value, json};

use crate::desk::Desk;
use crate::reach::Linked;
use crate::setup::{ITEM_AI, ITEM_BROKER, ITEM_DATA, ITEM_SYMBOLS};
use crate::{AiMethod, TeamWiring};

/// 키가 빠졌을 때 사람이 할 일.
const TODO_KEYS: &str =
    "이 컴퓨터의 설정 화면(또는 다른 기기에서 접속 키로 들어와 /keys)에서 등록합니다";
/// 지난 백업이 실패했을 때 사람이 할 일.
const TODO_BACKUP: &str = "설정 화면의 백업 칸에서 토큰 · 저장소를 확인합니다";
/// 저장소 판을 읽지 못했을 때 사람이 할 일.
const TODO_STORE: &str = "서버를 끈 뒤 price-inha doctor 를 실행해 저장소를 확인합니다";

/// 설정 화면이 읽는, 실행 중인 서버가 이미 아는 것: 설정 책상 · 켤 때 정한 시장 · 담당 모델 · 열린 저장소 ·
/// 3차 조립(백업). 띄울 때 한 번 만들고 바꾸지 않는다.
pub(crate) struct SetupView {
    desk: Arc<Desk>,
    market: Market,
    models: Vec<(&'static str, String)>,
    store: Store,
    linked: Option<Arc<Linked>>,
}

impl SetupView {
    pub(crate) fn new(
        desk: Arc<Desk>,
        market: Market,
        models: Vec<(&'static str, String)>,
        (store, linked): (Store, Option<Arc<Linked>>),
    ) -> Self {
        Self {
            desk,
            market,
            models,
            store,
            linked,
        }
    }

    /// 설정 책상.
    pub(crate) fn desk(&self) -> &Arc<Desk> {
        &self.desk
    }

    /// 설정 상태에 더하는 칸: `market` · `models`.
    pub(crate) fn add_to_status(&self, body: &mut Value) {
        body["market"] = json!(market_id(self.market));
        body["models"] = Value::Array(
            self.models
                .iter()
                .map(|(agent, model)| json!({ "agent": agent, "model": model }))
                .collect(),
        );
    }

    /// 진단 본문(`{"items": [...]}`, [`DIAGNOSIS_ITEMS`] 순서).
    pub(crate) fn diagnosis(&self) -> Value {
        let items: Vec<Value> = DIAGNOSIS_ITEMS
            .iter()
            .map(|key| self.check(key).to_json(key))
            .collect();
        json!({ "items": items })
    }

    fn check(&self, key: &str) -> Check {
        match key {
            "store" => self.check_store(),
            "keys" => self.check_keys(),
            "ai" => self.check_ai(),
            "ai_daily_limit" => self.check_ai_limit(),
            "backup" => self.check_backup(),
            "demo" => self.check_demo(),
            _ => Check::info("시장", market_label(self.market)),
        }
    }

    fn check_store(&self) -> Check {
        match self.store.info() {
            Ok(info) => Check::ok("저장소", format!("저장소 판 {}", info.schema_version)),
            Err(_) => Check::todo(
                DIAG_FAIL,
                "저장소",
                "저장소 판을 읽지 못했습니다",
                TODO_STORE,
            ),
        }
    }

    fn check_keys(&self) -> Check {
        let status = self.desk.status();
        let mut parts: Vec<String> = status
            .items
            .iter()
            // 종목은 키가 아니다(설정 화면의 종목 칸이 따로 보인다).
            .filter(|item| item.name != ITEM_SYMBOLS)
            .map(|item| format!("{} {}", item_label(item.name), presence(item.registered)))
            .collect();
        parts.extend(
            self.desk
                .slot_status()
                .iter()
                .map(|slot| format!("{} {}", slot_label(slot.slot), presence(slot.registered))),
        );
        let detail = parts.join(" · ");
        if self.desk.keys_ready() {
            Check::ok("키 등록", detail)
        } else {
            // 시연 모드는 키 없이 돌지만 실제로 쓰려면 키가 필요하다.
            let status = if status.demo { DIAG_WARN } else { DIAG_FAIL };
            Check::todo(
                status,
                "키 등록",
                format!("필수 키가 빠졌습니다: {detail}"),
                TODO_KEYS,
            )
        }
    }

    fn check_ai(&self) -> Check {
        let method = match self.desk.status().ai.method {
            AiMethod::ApiKey => "API 키",
            AiMethod::Cli => "Claude Code CLI",
        };
        Check::info("AI 연결", method)
    }

    fn check_ai_limit(&self) -> Check {
        let detail = match self.desk.shown_ai_limit() {
            Some(Some(limit)) => format!("{limit}번"),
            Some(None) => "없음".to_string(),
            None => "없음 (투자 팀을 잇지 않았습니다)".to_string(),
        };
        Check::info("하루 AI 호출 상한", detail)
    }

    fn check_backup(&self) -> Check {
        const LABEL: &str = "백업";
        let Some(backup) = self.linked.as_ref().and_then(|linked| linked.backup()) else {
            return Check::info(LABEL, "백업이 꺼져 있습니다");
        };
        let Ok(status) = backup.status() else {
            return Check::todo(
                DIAG_WARN,
                LABEL,
                "백업 켜짐 · 백업 상태를 읽지 못했습니다",
                TODO_BACKUP,
            );
        };
        let last = status.last_success.map(|at| kst_text(at.0));
        let last_text = last.as_ref().map_or_else(
            || "아직 백업한 적이 없습니다".to_string(),
            |at| format!("마지막 백업 {at}"),
        );
        if status.retry_pending {
            Check::todo(
                DIAG_WARN,
                LABEL,
                format!("백업 켜짐 · {last_text} · 지난 백업이 실패해 다시 올리기를 기다립니다"),
                TODO_BACKUP,
            )
        } else if last.is_some() {
            Check::ok(LABEL, format!("백업 켜짐 · {last_text}"))
        } else {
            Check::info(LABEL, format!("백업 켜짐 · {last_text}"))
        }
    }

    fn check_demo(&self) -> Check {
        let detail = if self.desk.demo() {
            "시연 모드로 돌고 있습니다(시연 시세 · 모의계좌)"
        } else {
            "실제 시세로 돌고 있습니다(주문은 모의계좌에만)"
        };
        Check::info("시연 모드", detail)
    }
}

/// 진단 항목 하나(`key`는 내보낼 때 붙인다).
struct Check {
    label: &'static str,
    status: &'static str,
    detail: String,
    todo: Option<&'static str>,
}

impl Check {
    fn ok(label: &'static str, detail: impl Into<String>) -> Self {
        Self::plain(DIAG_OK, label, detail)
    }

    fn info(label: &'static str, detail: impl Into<String>) -> Self {
        Self::plain(DIAG_INFO, label, detail)
    }

    fn plain(status: &'static str, label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            status,
            detail: detail.into(),
            todo: None,
        }
    }

    fn todo(
        status: &'static str,
        label: &'static str,
        detail: impl Into<String>,
        todo: &'static str,
    ) -> Self {
        Self {
            label,
            status,
            detail: detail.into(),
            todo: Some(todo),
        }
    }

    fn to_json(&self, key: &str) -> Value {
        json!({
            "key": key,
            "label": self.label,
            "status": self.status,
            "detail": self.detail,
            "todo": self.todo,
        })
    }
}

/// 담당마다 지금 쓰는 모델 이름([`Role::ALL`] 순서). 팀을 잇지 않으면 빈 목록이다.
pub(crate) fn models_of(team: Option<&TeamWiring>) -> Vec<(&'static str, String)> {
    team.map(|team| {
        Role::ALL
            .iter()
            .map(|role| (role.id(), team.model.clone()))
            .collect()
    })
    .unwrap_or_default()
}

/// 시장의 글자(`"kr"` · `"us"`).
fn market_id(market: Market) -> &'static str {
    match market {
        Market::Us => "us",
        _ => "kr",
    }
}

/// 진단에 보이는 시장 이름.
fn market_label(market: Market) -> &'static str {
    match market {
        Market::Us => "미국 (us)",
        _ => "국내 (kr)",
    }
}

fn presence(registered: bool) -> &'static str {
    if registered { "있음" } else { "없음" }
}

fn item_label(name: &str) -> &'static str {
    match name {
        ITEM_BROKER => "증권사 키",
        ITEM_AI => "AI 연결",
        ITEM_DATA => "자료 키(선택)",
        _ => "그 밖의 항목",
    }
}

fn slot_label(slot: KeySlot) -> &'static str {
    match slot {
        KeySlot::Dart => "DART 키(선택)",
        KeySlot::Ecos => "ECOS 키(선택)",
        KeySlot::Fred => "FRED 키(선택)",
        KeySlot::BackupGithubToken => "백업 GitHub 토큰(선택)",
        _ => "그 밖의 키(선택)",
    }
}

/// 한국 시각 `YYYY-MM-DD HH:MM`.
fn kst_text(at: i64) -> String {
    FixedOffset::east_opt(9 * 3600)
        .and_then(|kst| kst.timestamp_millis_opt(at).single())
        .map_or_else(
            || "알 수 없는 시각".to_string(),
            |t| t.format("%Y-%m-%d %H:%M").to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 할_일은_살펴볼_것과_문제에만_있다() {
        let ok = Check::ok("저장소", "판").to_json("store");
        assert_eq!(ok["todo"], Value::Null);
        let warn = Check::todo(DIAG_WARN, "키 등록", "없음", TODO_KEYS).to_json("keys");
        assert_eq!(warn["todo"], json!(TODO_KEYS));
        assert_eq!(warn["key"], json!("keys"));
    }

    #[test]
    fn 백업_시각은_한국_시각이다() {
        assert_eq!(kst_text(0), "1970-01-01 09:00");
    }

    #[test]
    fn 시장_글자() {
        assert_eq!(market_id(Market::Kr), "kr");
        assert_eq!(market_id(Market::Us), "us");
        assert!(market_label(Market::Us).contains("us"));
    }
}
