//! store 21차 시험(결정 31 — 오래된 기록을 덜어 낸다): 판 12, 보관으로 옮긴 범위만 지우는 좁은 길,
//! 감사 기록은 그대로, 쓰임 재기, 정수 양자화 임베딩, 공간 돌려받기.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    ArchiveCategory, ChunkId, KnowledgeKind, KnowledgeSource, NewArchiveBatch, NewKnowledgeRecord,
    NewRoomEvent, RoomActorKind, SCHEMA_VERSION, Store, StoreError, UnixMillis,
    archive_fingerprint, measure_file,
};
use serde_json::json;

const DAY: i64 = 86_400_000;
const BASE: i64 = 1_800_000_000_000;

fn at(day: i64) -> UnixMillis {
    UnixMillis(BASE + day * DAY)
}

fn copy_of_v11() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v11.db"),
        &path,
    )
    .unwrap();
    (dir, path)
}

fn count(path: &Path, sql: &str) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn raw(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

fn member_event(id: &str, day: i64) -> NewRoomEvent {
    NewRoomEvent {
        room: "team".into(),
        event_id: id.into(),
        kind: "message.member".into(),
        actor_kind: RoomActorKind::Member,
        actor_id: "scout".into(),
        payload: json!({"text": format!("글 {id}"), "trade": 3}),
        at: at(day),
    }
}

fn user_event(id: &str, day: i64) -> NewRoomEvent {
    NewRoomEvent {
        room: "team".into(),
        event_id: id.into(),
        kind: "message.user".into(),
        actor_kind: RoomActorKind::User,
        actor_id: "me".into(),
        payload: json!({"text": "사용자 글"}),
        at: at(day),
    }
}

/// 내보낸 것으로 보관 묶음을 만든다(파일은 시험에서 쓰지 않는다: 저장소는 파일을 보지 않는다).
fn batch_of(
    store: &Store,
    category: ArchiveCategory,
    from: i64,
    to: i64,
) -> (NewArchiveBatch, usize) {
    let export = store.archive_export(category, at(from), at(to)).unwrap();
    let rows = export.lines.len();
    let batch = NewArchiveBatch {
        category,
        period: format!("d{from}-{to}"),
        from_at: export.from_at,
        to_at: export.to_at,
        upto_rowid: export.upto_rowid,
        row_count: rows as u64,
        fingerprint: export.fingerprint.clone(),
        file: format!("{}-d{from}.jsonl", category.code()),
        file_sha256: "0".repeat(64),
        summary: json!({"rows": rows}).to_string(),
        created_at: at(to + 100),
    };
    (batch, rows)
}

// ───────────── 판 12 ─────────────

// 검증: D6 M1
#[test]
fn 새_저장소는_판_12다() {
    assert_eq!(SCHEMA_VERSION, 12);
    let (_dir, path, store) = temp_store();
    assert_eq!(store.info().unwrap().schema_version, 12);
    assert_eq!(count(&path, "PRAGMA user_version"), 12);
    assert!(store.archive_batches().unwrap().is_empty());
}

// 검증: D6 M1
#[test]
fn 판_11_파일을_올려도_기록은_그대로이고_보관_표가_생긴다() {
    let (_dir, path) = copy_of_v11();
    let before_events = count(&path, "SELECT COUNT(*) FROM events");
    let before_room = count(&path, "SELECT COUNT(*) FROM room_events");
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 12);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM events"), before_events);
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM room_events"),
        before_room
    );
    assert!(store.archive_batches().unwrap().is_empty());
    // 옛 f32 임베딩은 그대로 읽힌다.
    let emb = store
        .chunk_embedding(ChunkId(1))
        .unwrap()
        .expect("옛 임베딩");
    assert_eq!(emb.dims, 3);
    assert!(!emb.quantized);
    assert_eq!(emb.bytes.len(), 12);
    drop(store);
    let again = Store::open(&path).unwrap();
    assert_eq!(again.info().unwrap().schema_version, 12);
}

// 검증: M1 C11 V36
#[test]
fn 옛_파일에서도_보관하지_않은_줄은_sql로_지울_수_없다() {
    let (_dir, path) = copy_of_v11();
    drop(Store::open(&path).unwrap());
    let conn = raw(&path);
    for sql in [
        "DELETE FROM events",
        "DELETE FROM events WHERE kind = 'office.progress'",
        "DELETE FROM room_events",
        "DELETE FROM room_events WHERE kind = 'message.member'",
        "DELETE FROM access_log",
        "DELETE FROM team_history",
    ] {
        assert!(conn.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(count(&path, "SELECT COUNT(*) FROM events"), 19);
}

// ───────────── 좁은 지우기 길 ─────────────

// 검증: M1 C11 V36
#[test]
fn 화면_이벤트를_보관으로_옮기면_그_범위만_지워지고_감사_기록은_남는다() {
    let (_dir, path, store) = temp_store();
    for day in 0..5 {
        store
            .append_at(
                "office.progress",
                &json!({"agent": "scout", "trade": 9}),
                at(day),
            )
            .unwrap();
        store
            .append_at("rules_config", &json!({"changed": day}), at(day))
            .unwrap();
        store
            .append_at("office.agent", &json!({"agent": "lead"}), at(day))
            .unwrap();
    }
    store
        .append_at("office.progress", &json!({"late": true}), at(30))
        .unwrap();
    let (batch, rows) = batch_of(&store, ArchiveCategory::Screen, 0, 7);
    assert_eq!(rows, 10, "진행 · 걷기만 고른다(규칙 변경은 감사 기록)");
    let export = store
        .archive_export(ArchiveCategory::Screen, at(0), at(7))
        .unwrap();
    assert!(
        export
            .lines
            .iter()
            .all(|line| !line.contains("rules_config"))
    );
    assert_eq!(archive_fingerprint(&export.lines), export.fingerprint);
    let saved = store.archive_commit(batch.clone()).unwrap();
    assert_eq!(saved.batch, batch);
    assert_eq!(store.archive_batches().unwrap(), vec![saved]);
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM events WHERE kind = 'rules_config'"
        ),
        5
    );
    assert_eq!(
        count(
            &path,
            "SELECT COUNT(*) FROM events WHERE kind <> 'rules_config'"
        ),
        1,
        "기간 밖의 줄만 남는다"
    );
    // 좁은 길 밖은 여전히 막힌다.
    let conn = raw(&path);
    assert!(
        conn.execute("DELETE FROM events WHERE kind = 'rules_config'", [])
            .is_err()
    );
    assert!(conn.execute("DELETE FROM events", []).is_err());
    assert!(
        conn.execute("UPDATE archive_batches SET summary = 'x'", [])
            .is_err()
    );
    // 번호는 빠짐없이 이어진다.
    let next = store.append("chat", &json!({})).unwrap();
    assert_eq!(next, 17);
}

// 검증: M1 C11
#[test]
fn 지문이나_줄_수가_다르면_아무것도_지우지_않는다() {
    let (_dir, path, store) = temp_store();
    for day in 0..3 {
        store
            .append_at("team.ai_used", &json!({"agent": "lead"}), at(day))
            .unwrap();
    }
    store.append_at("team.ai_used", &json!({}), at(20)).unwrap();
    let (batch, rows) = batch_of(&store, ArchiveCategory::AiUsage, 0, 7);
    assert_eq!(rows, 3);
    let mut wrong = batch.clone();
    wrong.fingerprint = "0000000000000000".into();
    assert!(matches!(
        store.archive_commit(wrong),
        Err(StoreError::ArchiveMismatch(_))
    ));
    let mut short = batch.clone();
    short.row_count = 2;
    assert!(matches!(
        store.archive_commit(short),
        Err(StoreError::ArchiveMismatch(_))
    ));
    let mut early = batch;
    early.created_at = at(3);
    assert!(
        store.archive_commit(early).is_err(),
        "기간이 끝나기 전에 만들 수 없다"
    );
    assert_eq!(count(&path, "SELECT COUNT(*) FROM events"), 4);
    assert!(store.archive_batches().unwrap().is_empty());
}

// 검증: M1 C11
#[test]
fn 감사_종류와_감사_표는_어느_묶음에도_들지_않는다() {
    let audit = [
        "rules_config",
        "rule_proposal",
        "journal.review",
        "office.memo",
        "office.approval",
        "office.message",
        "promotion_check",
        "loss_basis",
        "reference_prices",
        "guideline",
        "day_close",
    ];
    for category in ArchiveCategory::ALL {
        for kind in audit {
            assert!(!category.kinds().contains(&kind), "{kind}");
        }
        assert!(["events", "room_events"].contains(&category.table()));
    }
    assert!(!ArchiveCategory::Room.kinds().contains(&"message.user"));
    assert!(ArchiveCategory::Room.kinds().contains(&"message.member"));
    assert!(ArchiveCategory::Screen.kinds().contains(&"office.progress"));
    assert_eq!(ArchiveCategory::AiUsage.kinds(), ["team.ai_used"]);
}

// 검증: M1 C11
#[test]
fn 표의_마지막_줄은_보관하지_않아_번호가_이어진다() {
    let (_dir, path, store) = temp_store();
    for day in 0..3 {
        store
            .append_at("office.progress", &json!({}), at(day))
            .unwrap();
    }
    let (batch, rows) = batch_of(&store, ArchiveCategory::Screen, 0, 7);
    assert_eq!(rows, 2, "가장 큰 번호는 남긴다");
    store.archive_commit(batch).unwrap();
    assert_eq!(count(&path, "SELECT MAX(seq) FROM events"), 3);
    let conn = raw(&path);
    assert!(
        conn.execute("DELETE FROM events WHERE seq = 3", [])
            .is_err()
    );
    assert_eq!(store.append("chat", &json!({})).unwrap(), 4);
}

// 검증: M1 C11
#[test]
fn 방_대화는_담당_글만_보관되고_사용자_글과_방의_마지막_줄은_남는다() {
    let (_dir, path, store) = temp_store();
    for day in 0..4 {
        store
            .append_room_event(member_event(&format!("m{day}"), day))
            .unwrap();
    }
    store.append_room_event(user_event("u1", 1)).unwrap();
    store.append_room_event(member_event("m9", 2)).unwrap();
    let (batch, rows) = batch_of(&store, ArchiveCategory::Room, 0, 7);
    assert_eq!(rows, 4, "담당 글 5개 가운데 방의 마지막 줄을 뺀다");
    store.archive_commit(batch).unwrap();
    assert_eq!(count(&path, "SELECT COUNT(*) FROM room_events"), 2);
    let appended = store.append_room_event(member_event("m10", 9)).unwrap();
    let _ = appended;
    assert_eq!(count(&path, "SELECT MAX(seq) FROM room_events"), 7);
    let conn = raw(&path);
    assert!(
        conn.execute("DELETE FROM room_events WHERE kind = 'message.user'", [])
            .is_err()
    );
}

// 검증: M1
#[test]
fn 가장_이른_덜어_낼_시각을_안다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(
        store.archive_oldest_at(ArchiveCategory::Screen).unwrap(),
        None
    );
    store.append_at("rules_config", &json!({}), t(0)).unwrap();
    store.append_at("office.agent", &json!({}), at(2)).unwrap();
    store.append_at("office.agent", &json!({}), at(3)).unwrap();
    assert_eq!(
        store.archive_oldest_at(ArchiveCategory::Screen).unwrap(),
        Some(at(2))
    );
    assert_eq!(
        store.archive_oldest_at(ArchiveCategory::Room).unwrap(),
        None
    );
}

// ───────────── 재기 · 공간 ─────────────

// 검증: D6
#[test]
fn 쓰임을_표와_종류마다_잰다() {
    let (_dir, path, store) = temp_store();
    for day in 0..4 {
        store
            .append_at("office.progress", &json!({"t": "x".repeat(100)}), at(day))
            .unwrap();
    }
    store.append_room_event(member_event("m1", 1)).unwrap();
    let usage = store.storage_usage().unwrap();
    assert!(usage.file_bytes() > 0);
    assert_eq!(usage.file_bytes(), usage.page_count * usage.page_size);
    let progress = usage
        .groups
        .iter()
        .find(|g| g.table == "events" && g.kind.as_deref() == Some("office.progress"))
        .expect("진행 묶음");
    assert_eq!(progress.rows, 4);
    assert!(progress.bytes >= 400);
    assert_eq!(progress.first_at, Some(at(0).0));
    assert_eq!(progress.last_at, Some(at(3).0));
    assert!(
        usage
            .groups
            .iter()
            .any(|g| g.table == "room_events" && g.kind.as_deref() == Some("message.member"))
    );
    assert!(
        usage
            .groups
            .iter()
            .any(|g| g.table == "trades" && g.kind.is_none())
    );
    drop(store);
    let measured = measure_file(&path).unwrap().expect("파일이 있다");
    assert_eq!(measured.groups.len(), usage.groups.len());
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(measure_file(&dir.path().join("없음.sqlite")).unwrap(), None);
}

// 검증: D6
#[test]
fn 옛_파일은_재도_판이_올라가지_않는다() {
    let (_dir, path) = copy_of_v11();
    let usage = measure_file(&path).unwrap().expect("잰다");
    assert!(
        usage
            .groups
            .iter()
            .any(|g| g.kind.as_deref() == Some("office.progress"))
    );
    assert_eq!(count(&path, "PRAGMA user_version"), 11);
}

// 검증: D6
#[test]
fn 덜어_낸_뒤_빈_쪽을_돌려받는다() {
    let (_dir, path, store) = temp_store();
    for day in 0..400 {
        store
            .append_at(
                "office.agent",
                &json!({"pad": "y".repeat(400)}),
                at(day % 6),
            )
            .unwrap();
    }
    let (batch, _) = batch_of(&store, ArchiveCategory::Screen, 0, 7);
    store.archive_commit(batch).unwrap();
    let first = store.reclaim_space(100_000).unwrap();
    assert!(first.free_after <= first.free_before);
    assert_eq!(first.free_after, 0);
    assert_eq!(
        count(&path, "PRAGMA auto_vacuum"),
        2,
        "incremental로 바뀐다"
    );
    let second = store.reclaim_space(10).unwrap();
    assert!(!second.converted, "한 번만 바꾼다");
}

// ───────────── 정수 양자화 임베딩 ─────────────

fn one_chunk(store: &Store) -> ChunkId {
    store
        .index_record(NewKnowledgeRecord {
            kind: KnowledgeKind::MarketNote,
            source: KnowledgeSource::Team,
            record_ref: "note-1".into(),
            chunks: vec!["조각 글".into()],
            at: at(0),
        })
        .unwrap()[0]
}

// 검증: D10 M7
#[test]
fn 정수_양자화_임베딩은_차원_더하기_4바이트이고_한_모양만_남는다() {
    let (_dir, path, store) = temp_store();
    let id = one_chunk(&store);
    for bytes in [&[][..], &[1, 2, 3][..], &[0, 0, 128, 63, 1, 2, 3, 4][..]] {
        assert!(matches!(
            store.set_chunk_embedding_q8(id, "m", 3, bytes),
            Err(StoreError::InvalidEmbedding)
        ));
    }
    assert!(matches!(
        store.set_chunk_embedding_q8(ChunkId(99), "m", 3, &[0, 0, 128, 63, 1, 2, 3]),
        Err(StoreError::ChunkNotFound(_))
    ));
    store
        .set_chunk_embedding(id, "m", 1, &[0, 0, 128, 63])
        .unwrap();
    store
        .set_chunk_embedding_q8(id, "m", 3, &[0, 0, 128, 63, 1, 254, 127])
        .unwrap();
    let emb = store.chunk_embedding(id).unwrap().expect("있다");
    assert!(emb.quantized);
    assert_eq!(emb.dims, 3);
    assert_eq!(emb.bytes, vec![0, 0, 128, 63, 1, 254, 127]);
    assert_eq!(count(&path, "SELECT COUNT(*) FROM knowledge_embeddings"), 0);
    assert!(store.chunks_missing_embedding("m", 10).unwrap().is_empty());
    assert_eq!(store.chunks_missing_embedding("n", 10).unwrap(), vec![id]);
    // f32로 다시 두면 양자화 줄은 지워진다.
    store
        .set_chunk_embedding(id, "m", 1, &[0, 0, 128, 63])
        .unwrap();
    assert!(!store.chunk_embedding(id).unwrap().unwrap().quantized);
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM knowledge_embeddings_q8"),
        0
    );
    // 조각을 다시 올리면 양자화 줄도 함께 지워진다.
    store
        .set_chunk_embedding_q8(id, "m", 3, &[0, 0, 128, 63, 1, 2, 3])
        .unwrap();
    store
        .index_record(NewKnowledgeRecord {
            kind: KnowledgeKind::MarketNote,
            source: KnowledgeSource::Team,
            record_ref: "note-1".into(),
            chunks: vec!["바뀐 글".into()],
            at: at(1),
        })
        .unwrap();
    assert_eq!(
        count(&path, "SELECT COUNT(*) FROM knowledge_embeddings_q8"),
        0
    );
}

// 검증: D10 M7
#[test]
fn 오래된_조각을_종류와_등급으로_고른다() {
    let (_dir, _path, store) = temp_store();
    for (day, source, name) in [
        (0, KnowledgeSource::External, "news-a"),
        (1, KnowledgeSource::Team, "note-b"),
        (2, KnowledgeSource::External, "news-c"),
        (9, KnowledgeSource::External, "news-d"),
    ] {
        store
            .index_record(NewKnowledgeRecord {
                kind: KnowledgeKind::MarketNote,
                source,
                record_ref: name.into(),
                chunks: vec![format!("{name} 글")],
                at: at(day),
            })
            .unwrap();
    }
    let old = store
        .knowledge_chunks_older(
            KnowledgeKind::MarketNote,
            KnowledgeSource::External,
            at(5),
            10,
        )
        .unwrap();
    let refs: Vec<&str> = old.iter().map(|c| c.record_ref.as_str()).collect();
    assert_eq!(refs, ["news-a", "news-c"]);
    let one = store
        .knowledge_chunks_older(
            KnowledgeKind::MarketNote,
            KnowledgeSource::External,
            at(5),
            1,
        )
        .unwrap();
    assert_eq!(one.len(), 1);
}
