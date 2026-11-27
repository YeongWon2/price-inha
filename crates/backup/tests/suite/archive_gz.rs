//! (6차, 결정 33) gzip 보관 파일(`*.jsonl.gz`)도 옛 무압축 `*.jsonl`과 함께 백업에 들고 되살리면 `archive/`로 돌아온다.
//! 등록된 비밀 값 검사는 풀어서 본 원문에 한다(압축된 바이트만 보면 비밀이 숨는다).

use std::io::Write;

use crate::common::*;
use pi_backup::{ARCHIVE_BUNDLE_PREFIX, BackupError, SNAPSHOT_FILE, restore};
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);
const LATER: UnixMillis = UnixMillis(NOW.0 + 3_600_000);
const OLD: &str = "room-2026-W06.jsonl";
const NEW: &str = "room-2026-W07.jsonl.gz";
const LINES: &[u8] = b"{\"kind\":\"message.member\"}\n";

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

// 검증: D7 V51
#[tokio::test]
async fn 압축_보관_파일과_옛_무압축_파일이_함께_들고_되살리면_돌아온다() {
    let env = Env::new().await;
    let dir = env.dir.path().join("archive");
    std::fs::create_dir_all(&dir).unwrap();
    let packed = gzip(LINES);
    std::fs::write(dir.join(OLD), LINES).unwrap();
    std::fs::write(dir.join(NEW), &packed).unwrap();
    std::fs::write(dir.join(".screen-2026-W07.jsonl.gz.tmp"), b"half").unwrap();
    std::fs::write(dir.join("notes.gz"), b"not an archive").unwrap();
    let report = env.backup().with_archive_dir(dir).run(NOW).await.unwrap();
    let paths: Vec<&str> = report
        .manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    let old = format!("{ARCHIVE_BUNDLE_PREFIX}{OLD}");
    let new = format!("{ARCHIVE_BUNDLE_PREFIX}{NEW}");
    assert_eq!(paths, [SNAPSHOT_FILE, old.as_str(), new.as_str()]);
    assert_eq!(report.manifest.files[2].sha256, sha_hex(&packed));

    let sealed = env.fake.file(&report.remote_path).unwrap();
    let target = env.target("restored");
    restore(&sealed, &env.identity_secret(), &target, LATER).unwrap();
    assert_eq!(
        std::fs::read(target.join("archive").join(OLD)).unwrap(),
        LINES
    );
    assert_eq!(
        std::fs::read(target.join("archive").join(NEW)).unwrap(),
        packed
    );
}

// 검증: E39 N8 V49
#[tokio::test]
async fn 압축_보관_파일_안에_등록된_비밀_값이_있으면_올리지_않는다() {
    let env = Env::new().await;
    let dir = env.dir.path().join("archive");
    std::fs::create_dir_all(&dir).unwrap();
    // 줄이 여럿이어야 압축된다(아주 짧으면 gzip이 원문을 그대로 담기도 한다).
    let mut text: String = (0..200)
        .map(|i| format!("{{\"kind\":\"message.member\",\"n\":{i}}}\n"))
        .collect();
    text.push_str(&format!("{{\"text\":\"키는 {DART_VALUE} 입니다\"}}\n"));
    let packed = gzip(text.as_bytes());
    assert!(
        !contains(&packed, DART_VALUE.as_bytes()),
        "압축하면 글자가 숨는다"
    );
    std::fs::write(dir.join(NEW), &packed).unwrap();
    let failure = env
        .backup()
        .with_archive_dir(dir)
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

// 검증: E39 D7
#[tokio::test]
async fn 풀리지_않는_압축_보관_파일은_올리지_않는다() {
    let env = Env::new().await;
    let dir = env.dir.path().join("archive");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(NEW), b"\x1f\x8b broken").unwrap();
    let failure = env
        .backup()
        .with_archive_dir(dir)
        .run(NOW)
        .await
        .unwrap_err();
    assert!(
        !matches!(failure.error, BackupError::SecretFound),
        "비밀이 아니라 읽지 못한 것이다: {:?}",
        failure.error
    );
    assert_eq!(env.fake.count("PUT").await, 0);
}
