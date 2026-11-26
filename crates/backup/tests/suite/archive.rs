//! (결정 31) 보관 파일(보관 폴더 `archive/`)도 백업에 들고, 되살리면 `archive/`로 돌아온다.

use crate::common::*;
use pi_backup::{ARCHIVE_BUNDLE_PREFIX, SNAPSHOT_FILE, restore};
use pi_store::UnixMillis;

const NOW: UnixMillis = UnixMillis(1_794_900_000_000);
const LATER: UnixMillis = UnixMillis(NOW.0 + 3_600_000);
const ROOM: &str = "room-2026-W07.jsonl";
const LINES: &[u8] = b"{\"kind\":\"message.member\"}\n{\"kind\":\"room.activity\"}\n";

fn archive_dir(env: &Env) -> std::path::PathBuf {
    let dir = env.dir.path().join("archive");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(ROOM), LINES).unwrap();
    std::fs::write(dir.join(".screen-2026-W07.jsonl.tmp"), b"half").unwrap();
    std::fs::write(dir.join("notes.txt"), b"not an archive").unwrap();
    dir
}

// 검증: D7 V51
#[tokio::test]
async fn 보관_파일이_백업에_들고_되살리면_보관_폴더로_돌아온다() {
    let env = Env::new().await;
    let dir = archive_dir(&env);
    let report = env.backup().with_archive_dir(dir).run(NOW).await.unwrap();
    let paths: Vec<&str> = report
        .manifest
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    let bundled = format!("{ARCHIVE_BUNDLE_PREFIX}{ROOM}");
    assert_eq!(
        paths,
        [SNAPSHOT_FILE, bundled.as_str()],
        "jsonl 보관 파일만 든다"
    );
    assert_eq!(report.manifest.files[1].sha256, sha_hex(LINES));

    let sealed = env.fake.file(&report.remote_path).unwrap();
    let target = env.target("restored");
    restore(&sealed, &env.identity_secret(), &target, LATER).unwrap();
    let back = target.join("archive").join(ROOM);
    assert_eq!(std::fs::read(&back).unwrap(), LINES);
    assert!(
        !target.join(&bundled).exists(),
        "묶음 이름 그대로 두지 않는다"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&back).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

// 검증: D7
#[tokio::test]
async fn 보관_폴더가_없으면_스냅샷만_든다() {
    let env = Env::new().await;
    let missing = env.dir.path().join("없는-보관");
    let report = env
        .backup()
        .with_archive_dir(missing)
        .run(NOW)
        .await
        .unwrap();
    assert_eq!(report.manifest.files.len(), 1);
    assert_eq!(report.manifest.files[0].path, SNAPSHOT_FILE);
}
