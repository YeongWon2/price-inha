//! (6차) 보관 폴더의 비밀이 아닌 설정 파일(하루 AI 상한 · 담당 모델 · 팀 지침 · 알림 설정 · 규칙 설정 …)도 백업에
//! 들고, 되살리면 제자리(대상 폴더 바로 아래)로 돌아온다. 키 파일(`pi_secrets::BACKUP_EXCLUDED_FILES`)은 이름을 줘도
//! 담지 않고, 담는 설정 파일도 등록된 비밀 값 검사를 거친다(E39).

use crate::common::*;
use pi_backup::{BackupError, SNAPSHOT_FILE, restore};
use pi_secrets::BACKUP_EXCLUDED_FILES;
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);
const LATER: UnixMillis = UnixMillis(NOW.0 + 3_600_000);
const LIMIT: &[u8] = b"{\"daily_limit\":40}";
const MODELS: &[u8] = b"{\"scout\":\"claude-haiku\"}";

fn home(env: &Env) -> std::path::PathBuf {
    env.dir.path().join("home")
}

fn names(report: &pi_backup::BackupReport) -> Vec<String> {
    report
        .manifest
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect()
}

// 검증: D7 V48 V51
#[tokio::test]
async fn 설정_파일이_백업에_들고_되살리면_제자리로_돌아온다() {
    let env = Env::new().await;
    let home = home(&env);
    std::fs::write(home.join("ai-limit.json"), LIMIT).unwrap();
    std::fs::write(home.join("team-models.json"), MODELS).unwrap();
    let report = env
        .backup()
        .with_config_files(
            home.clone(),
            &["ai-limit.json", "team-models.json", "없는-설정.json"],
        )
        .run(NOW)
        .await
        .unwrap();
    assert_eq!(
        names(&report),
        [SNAPSHOT_FILE, "ai-limit.json", "team-models.json"],
        "있는 설정 파일만 이름 그대로 든다"
    );

    let sealed = env.fake.file(&report.remote_path).unwrap();
    let target = env.target("restored");
    restore(&sealed, &env.identity_secret(), &target, LATER).unwrap();
    assert_eq!(std::fs::read(target.join("ai-limit.json")).unwrap(), LIMIT);
    assert_eq!(
        std::fs::read(target.join("team-models.json")).unwrap(),
        MODELS
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(target.join("ai-limit.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}

// 검증: E39 N8 D7
#[tokio::test]
async fn 키_파일과_저장소_이름은_설정으로_줘도_담지_않는다() {
    let env = Env::new().await;
    let home = home(&env);
    std::fs::write(home.join("ai-limit.json"), LIMIT).unwrap();
    let mut asked: Vec<&str> = BACKUP_EXCLUDED_FILES.to_vec();
    asked.extend([
        SNAPSHOT_FILE,
        "archive--room-2026-W07.jsonl",
        "../밖.json",
        "ai-limit.json",
    ]);
    let report = env
        .backup()
        .with_config_files(home.clone(), &asked)
        .run(NOW)
        .await
        .unwrap();
    assert_eq!(names(&report), [SNAPSHOT_FILE, "ai-limit.json"]);
}

// 검증: E39 N8 V49
#[tokio::test]
async fn 설정_파일에_등록된_비밀_값이_들면_올리지_않는다() {
    let env = Env::new().await;
    let home = home(&env);
    std::fs::write(
        home.join("notify-settings.json"),
        format!("{{\"memo\":\"{DART_VALUE}\"}}"),
    )
    .unwrap();
    let failure = env
        .backup()
        .with_config_files(home.clone(), &["notify-settings.json"])
        .run(NOW)
        .await
        .unwrap_err();
    assert!(
        matches!(failure.error, BackupError::SecretFound),
        "{:?}",
        failure.error
    );
    assert_eq!(env.fake.count("PUT").await, 0);
}

// 검증: D7
#[tokio::test]
async fn 설정_파일_이름을_주지_않으면_지금처럼_스냅샷만_든다() {
    let env = Env::new().await;
    std::fs::write(home(&env).join("ai-limit.json"), LIMIT).unwrap();
    let report = env.backup().run(NOW).await.unwrap();
    assert_eq!(names(&report), [SNAPSHOT_FILE]);
}
