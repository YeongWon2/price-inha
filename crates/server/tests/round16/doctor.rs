//! 진단: 항목 순서, 권한 · 깨진 저장소 · 루프백 밖 접속 키를 ✗와 고치기로, 키 값은 출력에 0번, 종료 코드.

use pi_store::{ACCESS_KEY_SALT_MIN_BYTES, AccessKeyHash, Store, UnixMillis};
use price_inha::{CheckItem, DB_FILE, DoctorReport, InstallOptions, Os, Service, doctor};

use crate::common::{
    AI_VALUE, BROKER_ID, BROKER_SECRET, FakeRunner, SLOT_VALUE, healthy_home, settings, setup,
};

/// `home`을 `bind`로 진단한다. 서비스는 macOS 모양으로 이 홈을 가리키게 등록해 둔다(가짜 실행기).
fn diagnose(home: &std::path::Path, bind: &str) -> DoctorReport {
    let defs = tempfile::tempdir().unwrap();
    let runner = FakeRunner::ok();
    let service = Service::new(setup(Os::MacOs, home, defs.path()), &runner);
    service
        .install(InstallOptions {
            start_now: false,
            dry_run: false,
        })
        .unwrap();
    doctor(&settings(home, bind), &service)
}

fn assert_no_key_text(text: &str) {
    for value in [BROKER_ID, BROKER_SECRET, AI_VALUE, SLOT_VALUE] {
        assert!(!text.contains(value), "키 값이 출력에 있다");
        assert!(!text.contains(&value[..4]), "키 앞 글자가 출력에 있다");
        assert!(
            !text.contains(&value[value.len() - 4..]),
            "키 뒤 글자가 출력에 있다"
        );
    }
}

// 검증: D1 D2
#[tokio::test]
async fn 건강한_보관_폴더는_문제가_없고_종료_코드_0() {
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    let report = diagnose(home.path(), "127.0.0.1");
    let items: Vec<CheckItem> = report.checks.iter().map(|c| c.item).collect();
    assert!(
        items.starts_with(&[
            CheckItem::Home,
            CheckItem::Database,
            CheckItem::Keys,
            CheckItem::Address,
            CheckItem::AccessKey,
            CheckItem::Service,
        ]),
        "지시서 순서대로 본다: {items:?}"
    );
    assert_eq!(report.problems(), 0, "{}", report.render());
    assert_eq!(report.exit_code(), 0);
    let rendered = report.render();
    for check in &report.checks {
        assert!(check.ok, "{check:?}");
    }
    assert!(rendered.lines().any(|l| l.starts_with('✓')), "{rendered}");
    assert!(!rendered.contains('✗'), "{rendered}");
}

// 검증: N8 D1
#[cfg(unix)]
#[tokio::test]
async fn 권한이_넓으면_고치기와_함께_문제이고_종료_코드_1() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let report = diagnose(home.path(), "127.0.0.1");
    let check = report.check(CheckItem::Home).expect("보관 폴더 항목");
    assert!(!check.ok, "0755는 문제");
    assert!(
        check
            .fix
            .as_deref()
            .is_some_and(|fix| !fix.trim().is_empty()),
        "고치기를 알려 준다: {check:?}"
    );
    assert_eq!(report.exit_code(), 1);
    let rendered = report.render();
    assert!(rendered.lines().any(|l| l.starts_with('✗')), "{rendered}");

    let mode = std::fs::metadata(home.path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "자동으로 고치지 않는다");
}

// 검증: N8
#[cfg(unix)]
#[tokio::test]
async fn 비밀_파일이_0600보다_넓으면_문제() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    let secret = home.path().join(pi_secrets::ENV_FILE);
    assert!(secret.exists(), "키를 등록하면 비밀 파일이 있다");
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).unwrap();
    let report = diagnose(home.path(), "127.0.0.1");
    let check = report.check(CheckItem::Home).expect("보관 폴더 항목");
    assert!(!check.ok, "비밀 파일 0644는 문제");
    assert!(check.fix.is_some());
    assert_eq!(report.exit_code(), 1);
    let mode = std::fs::metadata(&secret).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o644, "자동으로 고치지 않는다");
}

// 검증: D1
#[tokio::test]
async fn 깨진_저장소는_문제() {
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    std::fs::write(
        home.path().join(DB_FILE),
        b"this is plainly not a sqlite database file, only some words",
    )
    .unwrap();
    crate::common::narrow(home.path());
    let report = diagnose(home.path(), "127.0.0.1");
    let check = report.check(CheckItem::Database).expect("저장소 항목");
    assert!(!check.ok, "깨진 저장소는 문제");
    assert!(check.fix.is_some());
    assert_eq!(report.exit_code(), 1);
}

// 검증: N8
#[tokio::test]
async fn 키_값은_출력에_한_번도_나오지_않는다() {
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    let report = diagnose(home.path(), "127.0.0.1");
    assert_no_key_text(&report.render());
    assert_no_key_text(&report.to_json().to_string());
    assert_no_key_text(&format!("{report:?}"));
    let keys = report.check(CheckItem::Keys).expect("키 항목");
    assert!(keys.ok, "모두 등록돼 있다: {keys:?}");
}

// 검증: D2 N8
#[tokio::test]
async fn 루프백_밖에_여는데_접속_키가_없으면_문제() {
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    let report = diagnose(home.path(), "0.0.0.0");
    let check = report.check(CheckItem::AccessKey).expect("접속 키 항목");
    assert!(!check.ok, "D2: 밖으로 열면 접속 키가 있어야 한다");
    assert!(check.fix.is_some());
    assert_eq!(report.exit_code(), 1);

    let store = Store::open(home.path().join(DB_FILE)).unwrap();
    store
        .set_access_key(
            AccessKeyHash {
                sha256: [7; 32],
                salt: vec![3; ACCESS_KEY_SALT_MIN_BYTES],
            },
            UnixMillis(1_000),
        )
        .unwrap();
    drop(store);
    crate::common::narrow(home.path());
    let report = diagnose(home.path(), "0.0.0.0");
    let check = report.check(CheckItem::AccessKey).expect("접속 키 항목");
    assert!(check.ok, "접속 키가 있으면 된다: {check:?}");
}

// 검증: D1
#[tokio::test]
async fn json으로도_같은_결과를_낸다() {
    let home = tempfile::tempdir().unwrap();
    healthy_home(home.path()).await;
    std::fs::write(
        home.path().join(DB_FILE),
        b"broken broken broken broken broken",
    )
    .unwrap();
    crate::common::narrow(home.path());
    let report = diagnose(home.path(), "127.0.0.1");
    let json = report.to_json();
    let text = json.to_string();
    assert!(!text.is_empty());
    assert_eq!(report.exit_code(), 1);
    assert!(
        json.as_object().is_some() || json.as_array().is_some(),
        "JSON 값: {text}"
    );
    assert!(text.contains("false"), "문제 항목이 드러난다: {text}");
}
