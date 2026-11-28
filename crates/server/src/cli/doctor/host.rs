//! 진단 4~7: 이 컴퓨터의 주소 · 접속 키 · 서비스 · 백업.

use std::path::Path;

use chrono::{FixedOffset, TimeZone};
use serde_json::Value;

use super::{Check, CheckItem, bad, good};
use crate::cli::service::{SERVER_ERROR_LOG, SERVER_LOGS_DIR, Service};
use crate::{BACKUP_SETTINGS_FILE, Settings};

/// 진단 4: 여는 주소 · 포트를 쓸 수 있나. 이미 쓰는 것이 있으면 그 사실만 알린다.
pub(super) fn check_address(settings: &Settings) -> Check {
    let item = CheckItem::Address;
    let addr = std::net::SocketAddr::new(settings.bind, settings.port);
    match std::net::TcpListener::bind(addr) {
        Ok(listener) => {
            drop(listener);
            good(item, format!("{addr} 에 열 수 있습니다"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => good(
            item,
            format!("{addr} 를 이미 쓰는 것이 있습니다(서버가 도는 중이면 정상입니다)"),
        ),
        Err(error) => bad(
            item,
            format!("{addr} 에 열 수 없습니다: {}", error.kind()),
            "--bind · --port 를 이 컴퓨터에서 쓸 수 있는 값으로 바꿉니다",
        ),
    }
}

/// 진단 5: 루프백이 아닌 주소에 여는데 접속 키가 없으면 문제(D2). `has_key`가 `None`이면 저장소를 읽지
/// 못한 것이라(다른 프로그램이 쓰는 중) "없다"고 하지 않고 확인하지 못했다고만 알린다(거짓 ✗ 금지).
pub(super) fn check_access_key(settings: &Settings, has_key: Option<bool>) -> Check {
    let item = CheckItem::AccessKey;
    let bind = settings.bind;
    if bind.is_loopback() {
        return good(
            item,
            format!("{bind} 에만 엽니다(이 컴퓨터에서만 들어옵니다)"),
        );
    }
    match has_key {
        Some(true) => good(item, format!("{bind} 에 열고 접속 키가 있습니다")),
        None => good(
            item,
            format!(
                "{bind} 에 엽니다. 저장소를 읽지 못해 접속 키가 있는지 확인하지 못했습니다\
(서버를 멈춘 뒤 다시 실행하면 확인합니다)"
            ),
        ),
        Some(false) => bad(
            item,
            format!("루프백이 아닌 주소({bind})에 여는데 접속 키가 없습니다"),
            "price-inha access-key 로 접속 키를 만듭니다",
        ),
    }
}

/// 진단 6: 서비스 등록 여부와 정의가 가리키는 홈, Linux면 linger.
pub(super) fn check_service(home: &Path, service: &Service<'_>) -> Check {
    let item = CheckItem::Service;
    let status = match service.status() {
        Ok(status) => status,
        Err(error) => {
            return bad(
                item,
                format!("서비스 정의를 읽지 못했습니다: {error}"),
                "price-inha service uninstall 뒤 price-inha service install 로 다시 등록합니다",
            );
        }
    };
    let Some(installed) = status.home.clone() else {
        return good(
            item,
            "서비스로 등록하지 않았습니다(늘 돌리려면 price-inha service install)",
        );
    };
    if !same_place(&installed, home) {
        return bad(
            item,
            format!(
                "서비스 정의가 다른 보관 폴더를 가리킵니다: {}",
                installed.display()
            ),
            format!(
                "price-inha service uninstall 뒤 price-inha service install --home {} 로 다시 등록합니다",
                home.display()
            ),
        );
    }
    // (65차) macOS에서 계속 죽는 서비스: 기록 파일 자리를 알린다.
    if status.keeps_dying() {
        let code = status.last_exit_code.unwrap_or_default();
        let log = home.join(SERVER_LOGS_DIR).join(SERVER_ERROR_LOG);
        return bad(
            item,
            format!(
                "서비스가 계속 죽습니다(마지막 종료 코드 {code}, 서비스 관리자가 다시 띄우는 중)"
            ),
            format!(
                "{} 를 열어 까닭을 본 뒤 price-inha service restart 합니다",
                log.display()
            ),
        );
    }
    let running = if status.running {
        "도는 중"
    } else {
        "멈춤"
    };
    if service.linger() == Some(false) {
        return bad(
            item,
            format!("서비스 등록됨({running}) · linger 꺼짐(로그아웃하면 멈춥니다)"),
            "sudo loginctl enable-linger $USER",
        );
    }
    good(
        item,
        format!("서비스 등록됨({running}) · 이 보관 폴더를 가리킵니다"),
    )
}

fn same_place(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (std::fs::canonicalize(a), std::fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}

/// 백업 상태 파일을 두는 폴더(보관 폴더의 `backups/`, `pi-backup`이 쓴다 — 서버 조립의 `local_dir`). 비밀 값이 없다.
const BACKUP_DIR: &str = "backups";

/// 진단 7: 백업이 켜졌으면 마지막 백업 시각. 상태는 backup의 읽기 전용 상태 읽기([`pi_backup::read_status`])로 본다
/// (68차 백로그 U 2번 — 백업기를 만들지 않으므로 저장소를 열지 않는다).
pub(super) fn check_backup(home: &Path) -> Check {
    let item = CheckItem::Backup;
    let configured = std::fs::read_to_string(home.join(BACKUP_SETTINGS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .is_some_and(|value| {
            value["repo"].as_str().is_some_and(|s| !s.is_empty())
                && value["recipient"].as_str().is_some_and(|s| !s.is_empty())
        });
    if !configured {
        return good(item, "백업이 꺼져 있습니다");
    }
    let status = pi_backup::read_status(&home.join(BACKUP_DIR)).unwrap_or_default();
    let last = status.last_success.map(|at| kst_text(at.0));
    let pending = status.retry_pending;
    let last_text = last.map_or_else(
        || "아직 백업한 적이 없습니다".to_string(),
        |at| format!("마지막 백업 {at}"),
    );
    if pending {
        bad(
            item,
            format!("백업 켜짐 · {last_text} · 지난 백업이 실패해 다시 올리기를 기다립니다"),
            "설정 화면의 백업 칸에서 토큰 · 저장소를 확인합니다",
        )
    } else {
        good(item, format!("백업 켜짐 · {last_text}"))
    }
}

/// 한국 시각 `YYYY-MM-DD HH:MM`.
fn kst_text(at: i64) -> String {
    FixedOffset::east_opt(9 * 3600)
        .and_then(|kst| kst.timestamp_millis_opt(at).single())
        .map_or_else(
            || at.to_string(),
            |t| t.format("%Y-%m-%d %H:%M").to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 백업_시각은_한국_시각() {
        assert_eq!(kst_text(0), "1970-01-01 09:00");
    }
}
