//! 보관 폴더의 비밀이 아닌 설정 파일(하루 AI 상한 `ai-limit.json` · 담당 모델 · 팀 지침 · 알림 설정 · 규칙 설정 …)이
//! 백업 묶음에 들고, 되살리면 제자리로 돌아온다. 키 · 토큰 · 실행 증표 · 메신저 짝 맺기 장부는 들지 않는다.
//! 가짜 GitHub에 올라온 묶음을 시험 안에서 만든 age 키로 풀어 본다. 실제 GitHub는 부르지 않는다.

use price_inha::BACKUP_CONFIG_FILES;

use crate::common::{Bench, in_session};
use crate::github::{AgeKeys, FakeGithub};

/// 비밀이 아닌 설정 파일 이름들(이 차수가 백업에 넣는 것).
const CONFIG: [&str; 11] = [
    "ai-limit.json",
    "team-models.json",
    "team-guidelines.json",
    "notify-settings.json",
    "rules-config.json",
    "sound.json",
    "watchlist.json",
    "semantic-settings.json",
    "office-looks.json",
    "ai.json",
    "backup-settings.json",
];

/// 백업에 들면 안 되는 파일 이름들(키 · 토큰 · 실행 증표 · 짝 맺기 장부 · 메신저 위치). 키 파일 이름은 pi-secrets가 정한다.
const SECRET: [&str; 6] = [
    pi_secrets::ENV_FILE,
    pi_secrets::LEGACY_AUTH_FILE,
    "vapid.key",
    "run.proof",
    "messenger-pairing.json",
    "messenger-cursor.json",
];

// 검증: D7 E39 N8
#[test]
fn 백업에_넣는_설정_파일_목록은_비밀이_아닌_것뿐이다() {
    let mut listed: Vec<&str> = BACKUP_CONFIG_FILES.to_vec();
    listed.sort_unstable();
    let mut expected = CONFIG.to_vec();
    expected.sort_unstable();
    assert_eq!(listed, expected);
    for secret in SECRET {
        assert!(!BACKUP_CONFIG_FILES.contains(&secret), "{secret}");
    }
    for excluded in pi_secrets::BACKUP_EXCLUDED_FILES {
        assert!(!BACKUP_CONFIG_FILES.contains(excluded), "{excluded}");
    }
}

// 검증: D7 V48 V51 E39
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 설정_파일이_백업에_들고_되살리면_제자리로_돌아오고_비밀_파일은_들지_않는다() {
    let github = FakeGithub::start().await;
    let age = AgeKeys::generate();
    let bench = Bench::start_backing_up(in_session(), &github, &age).await;
    let home = bench.home().to_path_buf();
    for name in CONFIG {
        let path = home.join(name);
        if !path.exists() {
            std::fs::write(&path, format!("{{\"시험\":\"{name}\"}}")).unwrap();
        }
    }
    for name in [
        "messenger-pairing.json",
        "messenger-cursor.json",
        "run.proof",
    ] {
        let path = home.join(name);
        if !path.exists() {
            std::fs::write(&path, b"{}").unwrap();
        }
    }

    let report = bench.app().back_up_now().await.expect("백업");
    let names: Vec<String> = report
        .manifest
        .files
        .iter()
        .map(|f| f.path.clone())
        .collect();
    for name in CONFIG {
        assert!(names.iter().any(|n| n == name), "{name}가 든다: {names:?}");
    }
    for secret in SECRET {
        assert!(!names.iter().any(|n| n == secret), "{secret}는 들지 않는다");
    }

    let sealed = github.file(&report.remote_path);
    let bundle = pi_backup::open(&sealed, &age.identity).expect("우리 키로 풀린다");
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("restored");
    let at = pi_store::UnixMillis(report.at.0 + 1);
    pi_backup::restore(&sealed, &age.identity, &target, at).expect("되살린다");
    for name in CONFIG {
        let held = bundle.files.iter().find(|f| f.path == name).unwrap();
        assert_eq!(
            std::fs::read(target.join(name)).unwrap(),
            held.bytes,
            "{name}가 제자리로 돌아온다"
        );
    }
    for secret in SECRET {
        assert!(
            !target.join(secret).exists(),
            "{secret}는 되살아나지 않는다"
        );
    }
    bench.close().await;
}
