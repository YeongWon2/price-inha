//! 비밀 빼기: 키 파일은 대상이 아니고, 스냅샷에 키 글자가 보이면 올리지 않는다(E39 · N8 · V49 · P22).

use crate::common::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use pi_backup::{BackupError, FailureKind, SNAPSHOT_FILE};
use pi_secrets::BACKUP_EXCLUDED_FILES;
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);

/// 스냅샷에 `form`을 넣고 올려 본다. 거부돼야 한다.
async fn refused_with(form: &str) -> Env {
    let env = Env::new().await;
    assert!(
        env.secrets.contains_registered(form.as_bytes()),
        "전제: 비밀 검사가 알아본다"
    );
    pending_approval(&env.store, form, UnixMillis(NOW.0 - 1000));

    let failure = env.backup().run(NOW).await.unwrap_err();

    assert!(
        matches!(failure.error, BackupError::SecretFound),
        "{:?}",
        failure.error
    );
    assert_eq!(failure.notice.kind, FailureKind::SecretFound);
    assert_eq!(failure.notice.at, NOW);
    assert_eq!(env.fake.count("PUT").await, 0, "아무것도 올리지 않는다");
    assert!(env.fake.paths().is_empty());
    assert!(
        !names_in(&env.settings.local_dir)
            .iter()
            .any(|n| n.ends_with(".age"))
    );
    assert_eq!(names_in(&env.settings.temp_root), Vec::<String>::new());
    env
}

// 검증: V49, N8, P22
#[tokio::test]
async fn 올린_묶음에는_스냅샷만_있고_키_파일과_키_글자가_없다() {
    let env = Env::new().await;
    // 키 파일이 저장소와 같은 보관 폴더에 있다.
    let home = env.dir.path().join("home");
    assert!(
        names_in(&home)
            .iter()
            .any(|n| BACKUP_EXCLUDED_FILES.contains(&n.as_str()))
    );

    let report = env.backup().run(NOW).await.unwrap();

    let stored = env.fake.file(&report.remote_path).unwrap();
    assert!(!env.secrets.contains_registered(&stored));
    let bundle = pi_backup::open(&stored, &env.identity_secret()).unwrap();
    let paths: Vec<&str> = bundle.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec![SNAPSHOT_FILE]);
    for listed in &bundle.manifest.files {
        assert!(!BACKUP_EXCLUDED_FILES.contains(&listed.path.as_str()));
    }
    for held in &bundle.files {
        assert!(!env.secrets.contains_registered(&held.bytes));
        assert!(!contains(&held.bytes, DART_VALUE.as_bytes()));
        assert!(!contains(&held.bytes, TOKEN.as_bytes()));
    }
}

// 검증: E39, N8, V49
#[tokio::test]
async fn 등록된_값이_그대로_든_스냅샷은_올리지_않는다() {
    refused_with(&format!("메모: {DART_VALUE} 끝")).await;
}

// 검증: E39, N8
#[tokio::test]
async fn 등록된_값이_퍼센트_인코딩으로_들어도_올리지_않는다() {
    let encoded = utf8_percent_encode(DART_VALUE, NON_ALPHANUMERIC).to_string();
    assert_ne!(encoded, DART_VALUE);
    refused_with(&format!("query?k={encoded}")).await;
}

// 검증: E39, N8
#[tokio::test]
async fn 등록된_값이_베이스64로_들어도_올리지_않는다() {
    refused_with(&STANDARD.encode(DART_VALUE)).await;
}

// 검증: E39, N8, D7
#[tokio::test]
async fn 백업_토큰_값이_스냅샷에_들어도_올리지_않는다() {
    refused_with(TOKEN).await;
}

// 검증: P22, E39, E15
#[tokio::test]
async fn 거부해도_오류_알림_상태_기록에_키_글자가_없다() {
    let env = refused_with(DART_VALUE).await;
    let backup = env.backup();
    let failure = backup.run(UnixMillis(NOW.0 + 1)).await.unwrap_err();

    let shown = format!("{:?} {} {:?}", failure.error, failure.error, failure.notice);
    assert!(!shown.contains(DART_VALUE));
    let status = backup.status().unwrap();
    assert!(!format!("{status:?}").contains(DART_VALUE));
    assert!(status.retry_pending);
    assert_eq!(status.last_success, None);
    for bytes in all_bytes_under(&env.settings.local_dir) {
        assert!(!env.secrets.contains_registered(&bytes));
    }
}
