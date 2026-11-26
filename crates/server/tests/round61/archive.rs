//! 보관 기간이 지난 기록을 주별 요약 + 보관 파일로 바꾸고 DB에서 덜어 낸다. 감사 기록은 그대로다.

use std::path::Path;
use std::sync::Arc;

use pi_knowledge::{Knowledge, RecordText, SearchConfig};
use pi_store::{ArchiveCategory, KnowledgeKind, Store, UnixMillis};
use price_inha::{ARCHIVE_DIR, ARCHIVE_KEEP_DAYS_DEFAULT, ArchiveJob, DB_FILE, run_archive};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::common::{after_close, count, days_before, seed};

fn job(home: &Path, store: &Store, keep_days: u32) -> ArchiveJob {
    let knowledge = Knowledge::new(store.clone(), SearchConfig::default());
    ArchiveJob {
        store: store.clone(),
        knowledge: Some(Arc::new(knowledge)),
        home: home.to_path_buf(),
        keep_days,
    }
}

fn index_news(store: &Store, name: &str, at: UnixMillis) {
    let knowledge = Knowledge::new(store.clone(), SearchConfig::default());
    knowledge
        .index(RecordText::external(
            name,
            &format!("{name} 제목"),
            "뉴스 요약 글",
            "https://example.invalid/news",
            at,
        ))
        .unwrap();
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

// 검증: D6 M1 C11
#[test]
fn 보관_기간이_지난_대화와_화면_이벤트는_요약과_보관_파일로_바뀌고_감사_기록은_남는다() {
    assert_eq!(ARCHIVE_KEEP_DAYS_DEFAULT, 90);
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(DB_FILE);
    let store = Store::open(&path).unwrap();
    let now = after_close();
    let seeded = seed(&store, now);
    let before_events = count(&path, "SELECT COUNT(*) FROM events");
    let run = run_archive(&job(home.path(), &store, 90), now).expect("보관이 돈다");
    assert!(!run.periods.is_empty());

    // 팀 방: 옛 담당 글은 모두 빠지고(방의 마지막 줄은 최근 글이다) 사용자 글 · 최근 글은 남는다.
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM room_events WHERE kind = 'message.member'"
        ),
        4
    );
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM room_events WHERE kind = 'message.user'"
        ),
        3
    );
    // 감사 기록은 그대로다.
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM events WHERE kind = 'rules_config'"
        ),
        3
    );
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM events WHERE kind = 'journal.review'"
        ),
        3
    );
    let after_events = count(&path, "SELECT COUNT(*) FROM events");
    assert_eq!(
        before_events - after_events,
        (seeded.old_screen + seeded.old_ai) as i64,
        "옛 화면 이벤트와 AI 사용 세부만 빠진다"
    );
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM events WHERE kind = 'office.progress'"
        ),
        3,
        "최근 진행은 남는다"
    );

    // 보관 파일: 묶음마다 하나, 0600, 줄 수와 SHA-256이 남은 요약과 맞는다.
    let dir = home.path().join(ARCHIVE_DIR);
    #[cfg(unix)]
    assert_eq!(mode(&dir), 0o700);
    let batches = store.archive_batches().unwrap();
    assert_eq!(batches.len(), run.periods.len());
    let mut archived_rows = 0;
    for saved in &batches {
        let batch = &saved.batch;
        let file = dir.join(&batch.file);
        assert_eq!(
            batch.file,
            format!("{}-{}.jsonl", batch.category.code(), batch.period)
        );
        assert!(batch.period.contains("-W"), "주 이름: {}", batch.period);
        let bytes = std::fs::read(&file).expect("보관 파일");
        #[cfg(unix)]
        assert_eq!(mode(&file), 0o600);
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert_eq!(text.lines().count() as u64, batch.row_count);
        for line in text.lines() {
            let _: Value = serde_json::from_str(line).expect("줄마다 JSON");
        }
        assert_eq!(hex(&Sha256::digest(&bytes)), batch.file_sha256);
        let summary: Value = serde_json::from_str(&batch.summary).expect("요약은 JSON");
        assert_eq!(summary["rows"].as_u64(), Some(batch.row_count));
        assert!(summary["text"].as_str().unwrap().contains(&batch.period));
        archived_rows += batch.row_count;
    }
    assert_eq!(
        archived_rows as usize,
        seeded.old_member + seeded.old_screen + seeded.old_ai
    );
    let room: Vec<_> = batches
        .iter()
        .filter(|b| b.batch.category == ArchiveCategory::Room)
        .collect();
    assert!(!room.is_empty());
    let mut by_agent = 0;
    let mut trades = Vec::new();
    for saved in &room {
        let summary: Value = serde_json::from_str(&saved.batch.summary).unwrap();
        by_agent += summary["by_agent"]["scout"].as_u64().unwrap_or(0);
        for t in summary["trades"].as_array().unwrap() {
            trades.push(t.as_i64().unwrap());
        }
    }
    assert_eq!(by_agent, 10, "담당별 글 수");
    assert!(
        trades.contains(&3) && trades.contains(&4),
        "언급한 매매 번호: {trades:?}"
    );
}

// 검증: D6 M1
#[test]
fn 다시_돌리면_더_하는_것이_없고_보관_기간을_늘리면_아무것도_빼지_않는다() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(DB_FILE);
    let store = Store::open(&path).unwrap();
    let now = after_close();
    seed(&store, now);
    let long = run_archive(&job(home.path(), &store, 365), now).unwrap();
    assert!(long.periods.is_empty(), "1년 안의 기록은 그대로다");
    assert!(store.archive_batches().unwrap().is_empty());
    let first = run_archive(&job(home.path(), &store, 90), now).unwrap();
    assert!(!first.periods.is_empty());
    let reclaimed = first.reclaimed.expect("덜어 낸 뒤 공간을 돌려받는다");
    assert!(reclaimed.free_after <= reclaimed.free_before);
    assert_eq!(count(&path, "PRAGMA auto_vacuum"), 2);
    let batches = store.archive_batches().unwrap();
    let again = run_archive(&job(home.path(), &store, 90), now).unwrap();
    assert!(again.periods.is_empty());
    assert_eq!(again.knowledge_records, 0);
    assert_eq!(store.archive_batches().unwrap(), batches);
}

// 검증: D10 M7
#[test]
fn 오래된_바깥_자료_조각은_요약_기록_하나와_보관_파일로_바뀐다() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(DB_FILE);
    let store = Store::open(&path).unwrap();
    let now = after_close();
    for i in 0..3_i64 {
        index_news(
            &store,
            &format!("news-old-{i}"),
            UnixMillis(days_before(now, 120).0 + i),
        );
    }
    index_news(&store, "news-new", days_before(now, 5));
    let run = run_archive(&job(home.path(), &store, 90), now).unwrap();
    assert_eq!(run.knowledge_records, 3);
    for i in 0..3 {
        assert!(
            store
                .knowledge_chunks(KnowledgeKind::MarketNote, &format!("news-old-{i}"))
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(
        store
            .knowledge_chunks(KnowledgeKind::MarketNote, "news-new")
            .unwrap()
            .len(),
        1
    );
    let summaries = count(
        &path,
        "SELECT COUNT(*) FROM knowledge_chunks WHERE record_ref LIKE 'archive:knowledge:%'",
    );
    assert!(summaries >= 1);
    let dir = home.path().join(ARCHIVE_DIR);
    let files: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("knowledge-"))
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let text = std::fs::read_to_string(dir.join(&files[0])).unwrap();
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("news-old-0 제목"));
    #[cfg(unix)]
    assert_eq!(mode(&dir.join(&files[0])), 0o600);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
