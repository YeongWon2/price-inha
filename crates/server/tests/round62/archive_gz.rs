//! (결정 33) 보관 파일은 gzip(`archive/<종류>-<기간>.jsonl.gz`, 0600)이다. 줄 수 · SHA-256은 풀어서 본 원문 기준이고,
//! 옛 무압축 `.jsonl` 파일도 읽으며 덮지 않는다.

use std::path::Path;
use std::sync::Arc;

use pi_knowledge::{Knowledge, RecordText, SearchConfig};
use pi_store::{Store, UnixMillis};
use price_inha::{ARCHIVE_DIR, ArchiveJob, DB_FILE, read_archive_file, run_archive};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::common::{after_close, days_before, seed};

fn job(home: &Path, store: &Store) -> ArchiveJob {
    let knowledge = Knowledge::new(store.clone(), SearchConfig::default());
    ArchiveJob {
        store: store.clone(),
        knowledge: Some(Arc::new(knowledge)),
        home: home.to_path_buf(),
        keep_days: 90,
    }
}

fn index_old_news(store: &Store, at: UnixMillis) {
    let knowledge = Knowledge::new(store.clone(), SearchConfig::default());
    knowledge
        .index(RecordText::external(
            "news-old",
            "옛 뉴스 제목",
            "뉴스 요약 글",
            "https://example.invalid/news",
            at,
        ))
        .unwrap();
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 보관 폴더의 파일 이름들(숨은 파일 빼고, 이름 순서).
fn listed(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    names
}

fn archived_home() -> (tempfile::TempDir, Store) {
    let home = tempfile::tempdir().unwrap();
    let store = Store::open(home.path().join(DB_FILE)).unwrap();
    let now = after_close();
    seed(&store, now);
    index_old_news(&store, days_before(now, 110));
    (home, store)
}

// 검증: D6 M1 C11
#[test]
fn 보관_파일은_gzip이고_풀어_본_원문의_줄_수와_지문이_요약과_맞는다() {
    let (home, store) = archived_home();
    let run = run_archive(&job(home.path(), &store), after_close()).expect("보관이 돈다");
    assert!(!run.periods.is_empty());
    let dir = home.path().join(ARCHIVE_DIR);
    let names = listed(&dir);
    assert!(
        names.iter().any(|n| n.starts_with("knowledge-")),
        "바깥 자료 조각도 보관 파일로 간다: {names:?}"
    );
    for name in &names {
        assert!(name.ends_with(".jsonl.gz"), "gzip 이름: {name}");
        let raw = std::fs::read(dir.join(name)).unwrap();
        assert_eq!(&raw[..2], &[0x1f, 0x8b], "gzip 머리: {name}");
    }
    for saved in store.archive_batches().unwrap() {
        let batch = &saved.batch;
        assert_eq!(
            batch.file,
            format!("{}-{}.jsonl.gz", batch.category.code(), batch.period)
        );
        let file = dir.join(&batch.file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let text = read_archive_file(&file).expect("풀어 읽는다");
        assert_eq!(text.lines().count() as u64, batch.row_count);
        for line in text.lines() {
            let _: Value = serde_json::from_str(line).expect("줄마다 JSON");
        }
        assert_eq!(hex(&Sha256::digest(text.as_bytes())), batch.file_sha256);
        let packed = std::fs::metadata(&file).unwrap().len();
        assert!(packed > 0);
    }
}

// 검증: D6
#[test]
fn 옛_무압축_보관_파일도_읽는다() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("room-2026-W07.jsonl");
    std::fs::write(&old, "{\"a\":1}\n{\"b\":2}\n").unwrap();
    assert_eq!(read_archive_file(&old).unwrap(), "{\"a\":1}\n{\"b\":2}\n");
    let broken = dir.path().join("room-2026-W08.jsonl.gz");
    std::fs::write(&broken, b"\x1f\x8b nope").unwrap();
    assert!(read_archive_file(&broken).is_err(), "풀리지 않으면 오류다");
}

// 검증: D6
#[test]
fn 같은_기간의_옛_무압축_파일이_있으면_덮지_않고_다른_이름에_쓴다() {
    let (first, store) = archived_home();
    run_archive(&job(first.path(), &store), after_close()).unwrap();
    let made = listed(&first.path().join(ARCHIVE_DIR));

    let (second, store) = archived_home();
    let dir = second.path().join(ARCHIVE_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    let mut olds = Vec::new();
    for name in &made {
        let plain = name.trim_end_matches(".gz").to_owned();
        std::fs::write(dir.join(&plain), "옛 원문\n").unwrap();
        olds.push(plain);
    }
    run_archive(&job(second.path(), &store), after_close()).unwrap();
    for plain in &olds {
        assert_eq!(
            std::fs::read_to_string(dir.join(plain)).unwrap(),
            "옛 원문\n",
            "옛 파일은 그대로다: {plain}"
        );
    }
    for saved in store.archive_batches().unwrap() {
        let file = &saved.batch.file;
        assert!(file.ends_with("-2.jsonl.gz"), "옛 이름을 피한다: {file}");
        assert!(dir.join(file).exists());
    }
}
