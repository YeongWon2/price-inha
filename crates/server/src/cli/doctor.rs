//! 진단(`price-inha doctor`). 원본 동작: Hermes 2.6(범주를 순서대로, 사람이 할 것을 번호로, 문제가 남으면
//! 종료 코드 1) · OpenClaw 3.6(`--json`). 자동으로 고치지 않는다. 바깥(증권사 · AI · GitHub)을 부르지 않는다.

use std::path::{Path, PathBuf};

use pi_secrets::{KeyKind, KeySlot, SecretStore};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

use crate::ai_link::{AiMethod, load_ai_choice};
use crate::cli::peek::{access_key_generation, is_busy, open_read_only};
use crate::cli::service::Service;
use crate::{DB_FILE, Settings};

mod host;
mod ollama;

use host::{check_access_key, check_address, check_backup, check_service};
use ollama::check_ollama;

/// 진단 항목. 이 순서대로 본다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckItem {
    /// 보관 폴더가 있고 0700, 비밀 파일 0600(유닉스).
    Home,
    /// SQLite가 열리고 `PRAGMA integrity_check`가 ok, 스키마 버전을 안다.
    Database,
    /// 키 칸마다 등록 여부(이름만).
    Keys,
    /// 여는 주소 · 포트를 쓸 수 있나(이미 도는 서버가 있으면 그 사실만).
    Address,
    /// 루프백이 아닌 주소에 여는데 접속 키가 없으면 문제.
    AccessKey,
    /// 서비스 등록 여부와 정의가 가리키는 홈, Linux면 linger.
    Service,
    /// 백업이 켜졌으면 마지막 백업 시각.
    Backup,
    /// (10차 10c 13번) 뜻 검색(Ollama): 꺼짐이면 문제 아님(바깥 요청 0건), 켜졌으면 그 루프백 주소에 닿는지 ·
    /// 모델을 받아 두었는지.
    Ollama,
    /// (19차 3번) 하루 AI 호출 상한: 늘 문제 아님. 없으면 "없음", 있으면 "N번".
    AiDailyLimit,
}

/// 항목 하나의 결과. 키 값 · 길이 · 앞뒤 글자를 싣지 않는다(N8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// 항목.
    pub item: CheckItem,
    /// 문제가 없나.
    pub ok: bool,
    /// 무엇을 봤나(사람이 읽는 글).
    pub what: String,
    /// 문제면 사람이 할 일.
    pub fix: Option<String>,
}

/// 진단 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    /// 본 순서대로의 항목들.
    pub checks: Vec<Check>,
}

impl DoctorReport {
    /// 그 항목의 결과.
    pub fn check(&self, item: CheckItem) -> Option<&Check> {
        self.checks.iter().find(|check| check.item == item)
    }

    /// 문제(✗) 수.
    pub fn problems(&self) -> usize {
        self.checks.iter().filter(|check| !check.ok).count()
    }

    /// 종료 코드: 문제가 남으면 1, 아니면 0.
    pub fn exit_code(&self) -> u8 {
        u8::from(self.problems() > 0)
    }

    /// 항목마다 `✓ / ✗ 무엇 · 고치기` 한 줄, 끝에 사람이 할 것을 번호로.
    pub fn render(&self) -> String {
        let mut text = String::new();
        for check in &self.checks {
            match (&check.fix, check.ok) {
                (_, true) => text.push_str(&format!("✓ {}\n", check.what)),
                (Some(fix), false) => {
                    text.push_str(&format!("✗ {} · 고치기: {fix}\n", check.what));
                }
                (None, false) => text.push_str(&format!("✗ {}\n", check.what)),
            }
        }
        let todo: Vec<&str> = self
            .checks
            .iter()
            .filter(|check| !check.ok)
            .filter_map(|check| check.fix.as_deref())
            .collect();
        if todo.is_empty() {
            text.push_str("\n문제가 없습니다.\n");
        } else {
            text.push_str(&format!("\n문제 {}건. 할 일:\n", self.problems()));
            for (n, fix) in todo.iter().enumerate() {
                text.push_str(&format!("  {}. {fix}\n", n + 1));
            }
        }
        text
    }

    /// `--json`의 답.
    pub fn to_json(&self) -> serde_json::Value {
        let checks: Vec<Value> = self
            .checks
            .iter()
            .map(|check| {
                json!({
                    "item": item_name(check.item),
                    "ok": check.ok,
                    "what": check.what,
                    "fix": check.fix,
                })
            })
            .collect();
        json!({
            "ok": self.problems() == 0,
            "problems": self.problems(),
            "checks": checks,
        })
    }
}

/// JSON에 적는 항목 이름.
fn item_name(item: CheckItem) -> &'static str {
    match item {
        CheckItem::Home => "home",
        CheckItem::Database => "database",
        CheckItem::Keys => "keys",
        CheckItem::Address => "address",
        CheckItem::AccessKey => "access_key",
        CheckItem::Service => "service",
        CheckItem::Backup => "backup",
        CheckItem::Ollama => "ollama",
        CheckItem::AiDailyLimit => "ai_daily_limit",
    }
}

/// `settings`의 보관 폴더 · 주소 · 포트와 `service`의 등록 상태를 진단한다. 아무것도 고치지 않는다.
pub fn doctor(settings: &Settings, service: &Service<'_>) -> DoctorReport {
    let home = &settings.home;
    let home_check = check_home(home);
    let (database, has_access_key) = check_database(home);
    let keys = check_keys(home, home_check.ok);
    DoctorReport {
        checks: vec![
            home_check,
            database,
            keys,
            check_address(settings),
            check_access_key(settings, has_access_key),
            check_service(home, service),
            check_backup(home),
            check_ollama(home),
            check_ai_daily_limit(settings),
        ],
    }
}

/// (19차 3번) 진단: 하루 AI 호출 상한. 늘 문제 아님(✓).
fn check_ai_daily_limit(settings: &Settings) -> Check {
    let what = match settings.ai_daily_limit {
        None => "하루 AI 호출 상한 없음".to_string(),
        Some(limit) => format!("하루 AI 호출 상한 {limit}번"),
    };
    good(CheckItem::AiDailyLimit, what)
}

fn good(item: CheckItem, what: impl Into<String>) -> Check {
    Check {
        item,
        ok: true,
        what: what.into(),
        fix: None,
    }
}

fn bad(item: CheckItem, what: impl Into<String>, fix: impl Into<String>) -> Check {
    Check {
        item,
        ok: false,
        what: what.into(),
        fix: Some(fix.into()),
    }
}

/// 비밀이 들 수 있는 파일들(있는 것만 본다): 키 파일과 저장소 파일.
fn secret_files(home: &Path) -> Vec<PathBuf> {
    let db = DB_FILE.to_string();
    [
        pi_secrets::ENV_FILE.to_string(),
        pi_secrets::LEGACY_AUTH_FILE.to_string(),
        db.clone(),
        format!("{db}-wal"),
        format!("{db}-shm"),
    ]
    .into_iter()
    .map(|name| home.join(name))
    .filter(|path| path.exists())
    .collect()
}

/// 진단 1: 보관 폴더가 있고 0700, 비밀 파일 0600(유닉스).
fn check_home(home: &Path) -> Check {
    let item = CheckItem::Home;
    if !home.is_dir() {
        return bad(
            item,
            format!("보관 폴더가 없습니다: {}", home.display()),
            "price-inha 를 한 번 띄우면 만듭니다(또는 --home 으로 있는 폴더를 줍니다)",
        );
    }
    match wide_permissions(home) {
        Ok(None) => good(item, format!("보관 폴더 {} (권한 좁음)", home.display())),
        Ok(Some((what, fix))) => bad(item, what, fix),
        Err(kind) => bad(
            item,
            format!("보관 폴더 권한을 읽지 못했습니다: {kind}"),
            "보관 폴더를 읽을 수 있는 사용자로 실행합니다",
        ),
    }
}

/// 넓은 권한이 있으면 (무엇, 고치기).
#[cfg(unix)]
fn wide_permissions(home: &Path) -> Result<Option<(String, String)>, std::io::ErrorKind> {
    use std::os::unix::fs::PermissionsExt;
    let mode = |path: &Path| {
        std::fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o777)
            .map_err(|error| error.kind())
    };
    let mut what = Vec::new();
    let mut fix = Vec::new();
    let dir_mode = mode(home)?;
    if dir_mode & 0o077 != 0 {
        what.push(format!(
            "보관 폴더 권한이 {dir_mode:o}입니다(700이어야 합니다)"
        ));
        fix.push(format!("chmod 700 {}", home.display()));
    }
    let mut wide = Vec::new();
    for file in secret_files(home) {
        let file_mode = mode(&file)?;
        if file_mode & 0o077 != 0 {
            let name = file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            what.push(format!(
                "{name} 권한이 {file_mode:o}입니다(600이어야 합니다)"
            ));
            wide.push(file.display().to_string());
        }
    }
    if !wide.is_empty() {
        fix.push(format!("chmod 600 {}", wide.join(" ")));
    }
    if what.is_empty() {
        Ok(None)
    } else {
        Ok(Some((what.join(" · "), fix.join(" && "))))
    }
}

#[cfg(not(unix))]
fn wide_permissions(_home: &Path) -> Result<Option<(String, String)>, std::io::ErrorKind> {
    Ok(None)
}

/// 잠금 때문에 저장소를 확인하지 못했다. 깨진 것이 아니므로 문제로 세지 않는다.
fn in_use() -> Check {
    good(
        CheckItem::Database,
        "저장소를 다른 프로그램이 쓰는 중이라 확인하지 못했습니다(서버가 도는 중이면 정상입니다. \
잠시 뒤 다시 실행합니다)",
    )
}

/// 진단 2: 저장소가 열리고 `PRAGMA integrity_check`가 ok, 스키마 버전을 안다. 접속 키가 있는지도 함께 읽는다
/// (읽지 못했으면 `None` — "없다"가 아니라 "확인하지 못함"이다).
/// 새로 만들거나 이전하거나 권한을 바꾸지 않으려고 `pi_store::Store::open`이 아니라 있는 파일만 **읽기 전용**으로
/// 연다(9차 보강 2). 곁 파일이 없으면 `immutable=1`로 열어 아무 파일도 만들지 않는다(10차 10a 0번).
/// 잠금에 걸린 것은 깨진 것이 아니다: 다른 프로그램(도는 서버)이 쓰는 중이라고 알린다.
fn check_database(home: &Path) -> (Check, Option<bool>) {
    let item = CheckItem::Database;
    let path = home.join(DB_FILE);
    if !path.exists() {
        return (
            good(item, "저장소가 아직 없습니다(처음 띄울 때 만듭니다)"),
            Some(false),
        );
    }
    let restore_fix = "백업이 있으면 price-inha restore <백업 파일> --identity <비밀키 파일> --into <빈 폴더> \
로 되살린 뒤 그 폴더로 띄웁니다";
    let conn = match open_read_only(&path) {
        Ok(conn) => conn,
        Err(error) if is_busy(&error) => return (in_use(), None),
        Err(_) => return (bad(item, "저장소 파일을 열 수 없습니다", restore_fix), None),
    };
    let integrity: Result<String, _> =
        conn.query_row("PRAGMA integrity_check", [], |row| row.get(0));
    match integrity {
        Ok(text) if text == "ok" => {}
        Err(error) if is_busy(&error) => return (in_use(), None),
        _ => {
            let what = "저장소가 깨졌습니다(integrity_check 실패)";
            return (bad(item, what, restore_fix), None);
        }
    }
    let version: Result<i64, _> = conn.query_row("PRAGMA user_version", [], |row| row.get(0));
    let has_key = access_key_generation(&conn)
        .ok()
        .map(|found| found.is_some());
    let known = i64::from(pi_store::SCHEMA_VERSION);
    // (10차 10b 0번 다) 쓰는 템플릿이 멈춤이거나 없으면 매수가 막힌 까닭을 같은 ✓ 줄에 덧붙인다(문제로 세지 않는다).
    let notice = template_notice(&conn)
        .map(|notice| format!(" · {notice}"))
        .unwrap_or_default();
    let check = match version {
        Ok(v) if v == known => good(item, format!("저장소 정상(스키마 {v}){notice}")),
        Ok(v) if (0..known).contains(&v) => good(
            item,
            format!("저장소 정상(스키마 {v}, 띄울 때 {known}로 올립니다){notice}"),
        ),
        Ok(v) => bad(
            item,
            format!("저장소 스키마 {v}는 이 실행 파일({known})보다 새 판입니다"),
            "더 새 판의 price-inha 로 띄웁니다",
        ),
        Err(_) => bad(item, "저장소 스키마 버전을 읽지 못했습니다", restore_fix),
    };
    (check, has_key)
}

/// 쓰는 템플릿(가장 나중에 고른 것)이 없거나 멈춤이면 그 안내. 템플릿 표를 읽지 못하면(옛 스키마 등) 없음.
fn template_notice(conn: &rusqlite::Connection) -> Option<&'static str> {
    let state: Result<Option<String>, _> = conn
        .query_row(
            "SELECT state FROM templates ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional();
    match state {
        Ok(None) => Some(crate::trading::CHOOSE_TEMPLATE_NOTICE),
        Ok(Some(state)) if state == "stopped" => Some(crate::trading::CHOOSE_TEMPLATE_NOTICE),
        _ => None,
    }
}

fn kind_name(kind: KeyKind) -> &'static str {
    match kind {
        KeyKind::Broker => "증권사 키",
        KeyKind::AiApiKey => "AI API 키",
        KeyKind::Data => "자료 키(선택)",
    }
}

fn slot_name(slot: KeySlot) -> &'static str {
    match slot {
        KeySlot::Dart => "DART 키",
        KeySlot::Ecos => "ECOS 키",
        KeySlot::Fred => "FRED 키",
        KeySlot::BackupGithubToken => "백업 GitHub 토큰",
        _ => "그 밖의 키",
    }
}

fn provider_name(provider: &str) -> &str {
    match provider {
        "naver_news" => "네이버 뉴스",
        other => other,
    }
}

/// (다듬기 E 6번) 키 줄 끝에 덧붙이는 텔레그램 봇 토큰의 등록 여부(값 없이). 읽지 못하면 "없음"으로 본다.
fn telegram_part(secrets: &SecretStore) -> String {
    let registered = secrets
        .provider_status(&[crate::TELEGRAM_KEYS])
        .is_ok_and(|status| status.iter().any(|status| status.registered));
    let state = if registered { "있음" } else { "없음" };
    format!("텔레그램 봇 토큰 {state}")
}

/// 진단 3: 키 칸마다 등록 여부(이름만, 값 · 길이 · 앞뒤 글자 금지).
fn check_keys(home: &Path, home_ok: bool) -> Check {
    let item = CheckItem::Keys;
    if !home_ok {
        // 키 보관소를 열면 권한을 좁히므로(고치기), 보관 폴더 문제가 남아 있으면 열지 않는다.
        return bad(
            item,
            "보관 폴더 문제 때문에 키를 확인하지 않았습니다",
            "보관 폴더 문제를 먼저 고친 뒤 price-inha doctor 를 다시 실행합니다",
        );
    }
    let fix_keys =
        "이 컴퓨터의 설정 화면(또는 다른 기기에서 접속 키로 들어와 /keys)에서 등록합니다";
    let Ok(secrets) = SecretStore::open(home.to_path_buf()) else {
        return bad(item, "키 보관소를 열지 못했습니다", fix_keys);
    };
    let (Ok(kinds), Ok(slots)) = (secrets.status(), secrets.slot_status()) else {
        return bad(item, "키 파일을 읽지 못했습니다", fix_keys);
    };
    let cli = matches!(load_ai_choice(home), Ok(Some(choice)) if choice.method == AiMethod::Cli);
    let mut parts = Vec::new();
    let mut broker = false;
    let mut ai_key = false;
    for status in &kinds {
        // (다듬기 E 2번) 옛 한 칸짜리 자료 키는 더 받지 않는다(자료 키는 아래 칸들로 보인다).
        if matches!(status.kind, KeyKind::Data) {
            continue;
        }
        match status.kind {
            KeyKind::Broker => broker = status.registered,
            KeyKind::AiApiKey => ai_key = status.registered,
            KeyKind::Data => {}
        }
        let state = if status.registered {
            "있음"
        } else {
            "없음"
        };
        parts.push(format!("{} {state}", kind_name(status.kind)));
    }
    for status in &slots {
        let state = if status.registered {
            "있음"
        } else {
            "없음"
        };
        parts.push(format!("{} {state}", slot_name(status.slot)));
    }
    // (10b 10번) 공급자 선언의 키 칸(네이버 뉴스 등, 선택). 이름과 등록 여부만.
    let declared = crate::slots::provider_declarations();
    if let Ok(providers) = secrets.provider_status(&declared) {
        for status in &providers {
            let state = if status.registered {
                "있음"
            } else {
                "없음"
            };
            parts.push(format!(
                "{} 키(선택) {state}",
                provider_name(status.provider)
            ));
        }
    }
    if cli {
        parts.push("AI는 Claude Code CLI 사용".to_string());
    }
    parts.push(telegram_part(&secrets));
    let what = parts.join(" · ");
    if broker && (ai_key || cli) {
        good(item, what)
    } else {
        bad(item, format!("필수 키가 빠졌습니다: {what}"), fix_keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 문제가_없으면_종료_코드_0() {
        let report = DoctorReport {
            checks: vec![good(CheckItem::Home, "좋음")],
        };
        assert_eq!(report.exit_code(), 0);
        assert!(report.render().starts_with('✓'));
    }

    #[test]
    fn 할_일을_번호로_적는다() {
        let report = DoctorReport {
            checks: vec![
                bad(CheckItem::Home, "넓음", "chmod 700 x"),
                bad(CheckItem::Keys, "없음", "등록"),
            ],
        };
        let text = report.render();
        assert!(text.contains("1. chmod 700 x"), "{text}");
        assert!(text.contains("2. 등록"), "{text}");
        assert_eq!(report.exit_code(), 1);
        assert_eq!(report.to_json()["problems"], 2);
    }
}
