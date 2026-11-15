#![cfg(test)]
//! 11차: 지식(조각 · FTS5 trigram · 임베딩 · 메타) · 스냅샷 · 되살리기 표시 · 웹 푸시 구독 (D10 · M7 · V51).
//!
//! - 지식: 조각을 올리고 trigram으로 찾는다. 3글자 미만 낱말은 부분 문자열로 찾고, 바깥 자료는 자동 회상에서 뺀다.
//!   임베딩은 BLOB으로 두고 기록이 바뀌면 사라진다. 조각표를 `REPLACE`로 바꿔치려 하거나 SQL로 고쳐도
//!   색인이 어긋나지 않는다.
//! - 스냅샷: 백업 API로 한 파일에 뜨고, 그 파일을 열면 같은 자료다.
//! - 되살리기 표시: 팀이 멈추고 기다리던 승인이 만료된다(V51).
//! - 웹 푸시 구독: 같은 주소는 하나, 잘못된 값은 거부.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{
    ApprovalId, ApprovalStatus, ApprovalUse, ChunkId, DenyReason, KnowledgeKind, KnowledgeQuery,
    KnowledgeSource, NewApprovalRequest, NewKnowledgeRecord, NewPushSubscription, NewTemplate,
    SCHEMA_VERSION, Store, StoreError, TeamCause, TeamChange, TeamState, TemplateState, Transition,
};
use serde_json::json;

// ───────────── 도우미 ─────────────

fn rec(
    kind: KnowledgeKind,
    source: KnowledgeSource,
    record_ref: &str,
    chunks: &[&str],
    at: i64,
) -> NewKnowledgeRecord {
    NewKnowledgeRecord {
        kind,
        source,
        record_ref: record_ref.to_string(),
        chunks: chunks.iter().map(|c| (*c).to_string()).collect(),
        at: t(at),
    }
}

fn mine(record_ref: &str, chunks: &[&str]) -> NewKnowledgeRecord {
    rec(
        KnowledgeKind::Trade,
        KnowledgeSource::Mine,
        record_ref,
        chunks,
        0,
    )
}

fn query(text: &str) -> KnowledgeQuery {
    KnowledgeQuery {
        text: text.to_string(),
        kinds: Vec::new(),
        sources: Vec::new(),
        auto_recall: false,
        max: 50,
    }
}

fn found(store: &Store, text: &str) -> Vec<String> {
    store
        .search_knowledge(query(text))
        .unwrap()
        .into_iter()
        .map(|hit| hit.chunk.text)
        .collect()
}

fn raw(path: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open(path).unwrap()
}

fn count(path: &Path, sql: &str) -> i64 {
    raw(path).query_row(sql, [], |row| row.get(0)).unwrap()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn pending(store: &Store, fingerprint: &str) -> ApprovalId {
    store
        .create_approval(NewApprovalRequest {
            fingerprint: fingerprint.to_string(),
            summary: json!({ "주문": fingerprint }),
            created_at: t(0),
            expires_at: t(1_000_000),
        })
        .unwrap()
        .id
}

fn team(store: &Store, to: TeamState, cause: TeamCause, at: i64) {
    store
        .change_team_state(TeamChange {
            to,
            at: t(at),
            cause,
            detail: "시험".to_string(),
            owner_approved: false,
        })
        .unwrap();
}

fn live_template(store: &Store) -> pi_store::Template {
    let made = store
        .create_template(NewTemplate {
            value: json!({ "스타일": "스윙", "R%": "1.0", "하루_한도": "2", "손절": "고정" }),
            at: t(0),
        })
        .unwrap();
    store
        .submit_template_for_promotion(made.id, &json!({ "매매": 31, "거래일": 21 }), t(10))
        .unwrap();
    let approval = pending(store, "sha256:template-live");
    store
        .resolve_approval(approval, ApprovalStatus::Approved, t(11))
        .unwrap();
    store
        .promote_template_live(
            made.id,
            ApprovalUse {
                id: approval,
                fingerprint: "sha256:template-live".to_string(),
            },
            t(20),
        )
        .unwrap()
}

fn sub(endpoint: &str, at: i64) -> NewPushSubscription {
    NewPushSubscription {
        endpoint: endpoint.to_string(),
        p256dh: "공개키-시험".to_string(),
        auth: "인증-시험".to_string(),
        device: "폰".to_string(),
        at: t(at),
    }
}

/// 승인 대기가 있고 운용 중인 저장소를 한 파일로 뜬 것. (원본 폴더, 원본, 복사본 폴더, 복사본 경로)
fn snapped() -> (
    tempfile::TempDir,
    Store,
    tempfile::TempDir,
    PathBuf,
    ApprovalId,
) {
    let (dir, _path, store) = temp_store();
    team(&store, TeamState::Operating, TeamCause::TradingStart, 1);
    let waiting = pending(&store, "sha256:order-1");
    let copy_dir = tempfile::tempdir().unwrap();
    let copy = copy_dir.path().join("restored.sqlite");
    store.snapshot_to(&copy).unwrap();
    (dir, store, copy_dir, copy, waiting)
}

// ───────────── 지식: 색인 ─────────────

// 검증: D10 M7
#[test]
fn 스키마_버전은_7이고_번들_sqlite에서_fts5_trigram이_켜져_있다() {
    assert_eq!(SCHEMA_VERSION, 7);
    let (_dir, path, store) = temp_store();
    assert_eq!(store.info().unwrap().schema_version, 7);
    drop(store);
    assert_eq!(count(&path, "PRAGMA user_version"), 7);
    let mem = rusqlite::Connection::open_in_memory().unwrap();
    mem.execute_batch(
        "CREATE VIRTUAL TABLE probe USING fts5(body, tokenize='trigram');
         INSERT INTO probe(body) VALUES ('삼성전자 손절 기록');",
    )
    .unwrap();
    let hits: i64 = mem
        .query_row(
            "SELECT count(*) FROM probe WHERE probe MATCH '삼성전'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

// 검증: D10 M7
#[test]
fn 기록을_올리면_조각이_순서대로_번호와_함께_읽힌다() {
    let (_dir, _path, store) = temp_store();
    let ids = store
        .index_record(rec(
            KnowledgeKind::Review,
            KnowledgeSource::Team,
            "복기-7",
            &["첫 조각 글", "둘째 조각 글", "셋째 조각 글"],
            5,
        ))
        .unwrap();
    assert_eq!(ids, vec![ChunkId(1), ChunkId(2), ChunkId(3)]);
    let chunks = store
        .knowledge_chunks(KnowledgeKind::Review, "복기-7")
        .unwrap();
    assert_eq!(chunks.len(), 3);
    for (i, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.id, ids[i]);
        assert_eq!(chunk.kind, KnowledgeKind::Review);
        assert_eq!(chunk.source, KnowledgeSource::Team);
        assert_eq!(chunk.record_ref, "복기-7");
        assert_eq!(chunk.ordinal, u32::try_from(i).unwrap());
        assert_eq!(chunk.updated_at, t(5));
    }
    assert_eq!(chunks[1].text, "둘째 조각 글");
    assert!(
        store
            .knowledge_chunks(KnowledgeKind::Trade, "복기-7")
            .unwrap()
            .is_empty()
    );
}

// 검증: D10 M7
#[test]
fn 같은_기록을_다시_올리면_옛_조각이_사라지고_다른_기록은_그대로다() {
    let (_dir, _path, store) = temp_store();
    store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    store
        .index_record(mine("매매-2", &["삼성전자 매수 신호"]))
        .unwrap();
    let again = store
        .index_record(mine("매매-1", &["하이닉스 익절 기록", "하이닉스 복기"]))
        .unwrap();
    assert_eq!(again, vec![ChunkId(3), ChunkId(4)]);
    assert_eq!(found(&store, "손절 기록"), Vec::<String>::new());
    assert_eq!(
        found(&store, "하이닉스"),
        vec!["하이닉스 익절 기록", "하이닉스 복기"]
    );
    assert_eq!(found(&store, "삼성전자"), vec!["삼성전자 매수 신호"]);
    let old = store
        .knowledge_chunks(KnowledgeKind::Trade, "매매-1")
        .unwrap();
    assert_eq!(old.len(), 2);
    assert_eq!(old[0].id, ChunkId(3));
}

// 검증: D10 M7
#[test]
fn 조각이_없는_기록을_올리면_그_기록의_조각이_모두_지워진다() {
    let (_dir, _path, store) = temp_store();
    store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    store
        .index_record(mine("매매-2", &["삼성전자 매수 신호"]))
        .unwrap();
    assert_eq!(
        store.index_record(mine("매매-1", &[])).unwrap(),
        Vec::<ChunkId>::new()
    );
    assert!(
        store
            .knowledge_chunks(KnowledgeKind::Trade, "매매-1")
            .unwrap()
            .is_empty()
    );
    assert_eq!(found(&store, "삼성전자"), vec!["삼성전자 매수 신호"]);
}

// 검증: D10 M7
#[test]
fn 규칙에_어긋난_기록은_오류이고_아무것도_바뀌지_않는다() {
    let (_dir, _path, store) = temp_store();
    store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    for bad in [
        mine("", &["글"]),
        mine("매매-1", &["새 글", ""]),
        mine("매매-9", &["좋은 글", ""]),
    ] {
        let err = store.index_record(bad).unwrap_err();
        assert!(matches!(err, StoreError::InvalidKnowledge(_)), "{err:?}");
    }
    let kept = store
        .knowledge_chunks(KnowledgeKind::Trade, "매매-1")
        .unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].text, "삼성전자 손절 기록");
    assert_eq!(kept[0].id, ChunkId(1));
    assert!(
        store
            .knowledge_chunks(KnowledgeKind::Trade, "매매-9")
            .unwrap()
            .is_empty()
    );
    assert_eq!(found(&store, "새 글"), Vec::<String>::new());
    // 실패한 호출이 번호를 쓰지 않는다.
    let next = store
        .index_record(mine("매매-3", &["다음 글 조각"]))
        .unwrap();
    assert_eq!(next, vec![ChunkId(2)]);
}

// ───────────── 지식: 검색 ─────────────

// 검증: D10 M7
#[test]
fn 세_글자_이상_낱말은_trigram으로_찾아_점수가_있고_두_글자_낱말은_부분_문자열로_찾아_점수가_없다()
{
    let (_dir, _path, store) = temp_store();
    store
        .index_record(mine(
            "매매-1",
            &["삼성전자 손절 기록", "하이닉스 매수 신호"],
        ))
        .unwrap();
    let long = store.search_knowledge(query("삼성전")).unwrap();
    assert_eq!(long.len(), 1);
    assert_eq!(long[0].chunk.text, "삼성전자 손절 기록");
    let score = long[0].score.expect("trigram 적중은 점수가 있다");
    assert!(score > 0.0 && score <= 1.0, "{score}");

    let short = store.search_knowledge(query("매수")).unwrap();
    assert_eq!(short.len(), 1);
    assert_eq!(short[0].chunk.text, "하이닉스 매수 신호");
    assert_eq!(short[0].score, None);
}

// 검증: D10 M7
#[test]
fn 낱말이_모두_들어_있는_조각만_나온다() {
    let (_dir, _path, store) = temp_store();
    store
        .index_record(mine(
            "매매-1",
            &[
                "삼성전자 손절 기록",
                "삼성전자 매수 신호",
                "하이닉스 손절 기록",
            ],
        ))
        .unwrap();
    assert_eq!(found(&store, "삼성전자 손절"), vec!["삼성전자 손절 기록"]);
    assert_eq!(found(&store, "삼성전자 손절 신호"), Vec::<String>::new());
    assert_eq!(
        found(&store, "손절 기록 삼성전자"),
        vec!["삼성전자 손절 기록"]
    );
    assert_eq!(
        found(&store, "  삼성전자   매수  "),
        vec!["삼성전자 매수 신호"]
    );
}

// 검증: D10 M7
#[test]
fn 영문은_대소문자를_가리지_않는다() {
    let (_dir, _path, store) = temp_store();
    store
        .index_record(mine("매매-1", &["NVDA Earnings Beat"]))
        .unwrap();
    assert_eq!(found(&store, "earnings"), vec!["NVDA Earnings Beat"]);
    assert_eq!(found(&store, "nvda beat"), vec!["NVDA Earnings Beat"]);
    assert_eq!(found(&store, "nv"), vec!["NVDA Earnings Beat"]);
}

// 검증: D10 M7
#[test]
fn 질의의_검색_문법_글자는_글자_그대로_찾는다() {
    let (_dir, _path, store) = temp_store();
    store
        .index_record(mine(
            "매매-1",
            &["그는 \"손절\"이라 했다", "삼성전자 하이닉스"],
        ))
        .unwrap();
    assert_eq!(found(&store, "\"손절\""), vec!["그는 \"손절\"이라 했다"]);
    for literal in [
        "OR",
        "NOT",
        "삼성전자*",
        "삼성전자 OR 하이닉스",
        "NEAR(삼성전자 하이닉스)",
    ] {
        assert_eq!(found(&store, literal), Vec::<String>::new(), "{literal}");
    }
    assert!(store.search_knowledge(query("\"")).is_ok());
}

// 검증: D10 M7
#[test]
fn 종류와_등급으로_거르고_개수를_자른다() {
    let (_dir, _path, store) = temp_store();
    let all = [
        (KnowledgeKind::Trade, KnowledgeSource::Mine, "나-매매"),
        (KnowledgeKind::Review, KnowledgeSource::Team, "팀-복기"),
        (
            KnowledgeKind::Mistake,
            KnowledgeSource::External,
            "바깥-실수",
        ),
        (KnowledgeKind::MarketNote, KnowledgeSource::Mine, "나-메모"),
    ];
    for (kind, source, name) in all {
        store
            .index_record(rec(kind, source, name, &[&format!("공통낱말 {name}")], 0))
            .unwrap();
    }
    assert_eq!(found(&store, "공통낱말").len(), 4);
    let mut by_kind = query("공통낱말");
    by_kind.kinds = vec![KnowledgeKind::Review, KnowledgeKind::MarketNote];
    assert_eq!(store.search_knowledge(by_kind).unwrap().len(), 2);
    let mut by_source = query("공통낱말");
    by_source.sources = vec![KnowledgeSource::Mine];
    let mine_only = store.search_knowledge(by_source).unwrap();
    assert_eq!(mine_only.len(), 2);
    assert!(
        mine_only
            .iter()
            .all(|h| h.chunk.source == KnowledgeSource::Mine)
    );
    let mut both = query("공통낱말");
    both.kinds = vec![KnowledgeKind::Trade, KnowledgeKind::Review];
    both.sources = vec![KnowledgeSource::Mine];
    assert_eq!(store.search_knowledge(both).unwrap().len(), 1);
    let mut limited = query("공통낱말");
    limited.max = 3;
    assert_eq!(store.search_knowledge(limited).unwrap().len(), 3);
    let mut zero = query("공통낱말");
    zero.max = 0;
    assert!(store.search_knowledge(zero).unwrap().is_empty());
    assert!(store.search_knowledge(query("   ")).unwrap().is_empty());
}

// 검증: D10 M7
#[test]
fn 자동_회상에서는_바깥_자료가_등급을_지정해도_나오지_않는다() {
    let (_dir, _path, store) = temp_store();
    store
        .index_record(rec(
            KnowledgeKind::MarketNote,
            KnowledgeSource::External,
            "뉴스-1",
            &["바깥 뉴스 삼성전자"],
            0,
        ))
        .unwrap();
    store
        .index_record(rec(
            KnowledgeKind::MarketNote,
            KnowledgeSource::Team,
            "메모-1",
            &["팀 메모 삼성전자"],
            0,
        ))
        .unwrap();
    let mut recall = query("삼성전자");
    recall.auto_recall = true;
    let hits = store.search_knowledge(recall.clone()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].chunk.source, KnowledgeSource::Team);
    recall.sources = vec![KnowledgeSource::External];
    assert!(store.search_knowledge(recall).unwrap().is_empty());
    let mut manual = query("삼성전자");
    manual.sources = vec![KnowledgeSource::External];
    assert_eq!(store.search_knowledge(manual).unwrap().len(), 1);
    assert_eq!(found(&store, "삼성전자").len(), 2);
}

// ───────────── 지식: 임베딩 · 메타 ─────────────

// 검증: D10 M7
#[test]
fn 임베딩은_비트까지_그대로_왕복하고_블롭에_리틀_엔디언으로_들어간다() {
    let (_dir, path, store) = temp_store();
    let ids = store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    assert_eq!(store.chunk_embedding(ids[0]).unwrap(), None);
    let vector = [
        0.1_f32,
        -0.0,
        1.5e-30,
        f32::MIN_POSITIVE,
        -123.456,
        f32::MAX,
    ];
    store
        .set_chunk_embedding(ids[0], "시험-모델", &vector)
        .unwrap();
    let got = store.chunk_embedding(ids[0]).unwrap().unwrap();
    assert_eq!(got.model, "시험-모델");
    let bits = |v: &[f32]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
    assert_eq!(bits(&got.vector), bits(&vector));
    let blob: Vec<u8> = raw(&path)
        .query_row(
            "SELECT embedding FROM knowledge_embeddings WHERE chunk_id = ?1",
            [ids[0].0],
            |row| row.get(0),
        )
        .unwrap();
    let expected: Vec<u8> = vector.iter().flat_map(|x| x.to_le_bytes()).collect();
    assert_eq!(blob, expected);
    assert_eq!(count(&path, "SELECT dims FROM knowledge_embeddings"), 6);
    store
        .set_chunk_embedding(ids[0], "다른-모델", &[1.0, 2.0])
        .unwrap();
    let again = store.chunk_embedding(ids[0]).unwrap().unwrap();
    assert_eq!(
        (again.model.as_str(), again.vector.as_slice()),
        ("다른-모델", &[1.0, 2.0][..])
    );
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_embeddings"), 1);
}

// 검증: D10 M7
#[test]
fn 잘못된_임베딩은_거부하고_없는_조각의_임베딩은_오류다() {
    let (_dir, _path, store) = temp_store();
    let ids = store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    for bad in [
        vec![],
        vec![1.0, f32::NAN],
        vec![f32::INFINITY],
        vec![f32::NEG_INFINITY, 0.0],
    ] {
        let err = store.set_chunk_embedding(ids[0], "모델", &bad).unwrap_err();
        assert!(matches!(err, StoreError::InvalidEmbedding), "{err:?}");
    }
    assert_eq!(store.chunk_embedding(ids[0]).unwrap(), None);
    let err = store
        .set_chunk_embedding(ChunkId(99), "모델", &[1.0])
        .unwrap_err();
    assert!(
        matches!(err, StoreError::ChunkNotFound(ChunkId(99))),
        "{err:?}"
    );
    assert_eq!(store.chunk_embedding(ChunkId(99)).unwrap(), None);
}

// 검증: D10 M7
#[test]
fn 기록이_바뀌면_임베딩이_사라져_다시_만들_목록에_나온다() {
    let (_dir, path, store) = temp_store();
    let first = store
        .index_record(mine("매매-1", &["가나다 조각", "라마바 조각"]))
        .unwrap();
    let other = store
        .index_record(mine("매매-2", &["사아자 조각"]))
        .unwrap();
    for id in first.iter().chain(&other) {
        store.set_chunk_embedding(*id, "m1", &[0.5, 0.25]).unwrap();
    }
    assert!(store.chunks_missing_embedding("m1", 10).unwrap().is_empty());
    assert_eq!(
        store.chunks_missing_embedding("m2", 10).unwrap(),
        vec![first[0], first[1], other[0]]
    );
    assert_eq!(
        store.chunks_missing_embedding("m2", 2).unwrap(),
        vec![first[0], first[1]]
    );
    assert!(store.chunks_missing_embedding("m2", 0).unwrap().is_empty());

    let renewed = store
        .index_record(mine("매매-1", &["가나다 조각", "차카타 조각"]))
        .unwrap();
    assert_eq!(store.chunk_embedding(first[0]).unwrap(), None);
    assert!(store.chunk_embedding(other[0]).unwrap().is_some());
    assert_eq!(store.chunks_missing_embedding("m1", 10).unwrap(), renewed);
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_embeddings"), 1);
}

// 검증: D10 M7
#[test]
fn 메타는_두고_바꾸고_읽고_빈_키는_거부한다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(store.knowledge_meta("provider").unwrap(), None);
    store.set_knowledge_meta("provider", "없음").unwrap();
    store.set_knowledge_meta("model", "시험-모델").unwrap();
    store.set_knowledge_meta("provider", "나중-공급자").unwrap();
    assert_eq!(
        store.knowledge_meta("provider").unwrap().as_deref(),
        Some("나중-공급자")
    );
    assert_eq!(
        store.knowledge_meta("model").unwrap().as_deref(),
        Some("시험-모델")
    );
    let err = store.set_knowledge_meta("", "값").unwrap_err();
    assert!(matches!(err, StoreError::InvalidKnowledge(_)), "{err:?}");
}

// 검증: D10 M7
#[test]
fn 다시_열어도_조각_임베딩_메타가_그대로이고_검색이_된다() {
    let (_dir, path, store) = temp_store();
    let ids = store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    store
        .set_chunk_embedding(ids[0], "m1", &[1.0, 2.0, 3.0])
        .unwrap();
    store.set_knowledge_meta("model", "m1").unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(found(&reopened, "삼성전자"), vec!["삼성전자 손절 기록"]);
    assert_eq!(
        reopened.chunk_embedding(ids[0]).unwrap().unwrap().vector,
        vec![1.0, 2.0, 3.0]
    );
    assert_eq!(
        reopened.knowledge_meta("model").unwrap().as_deref(),
        Some("m1")
    );
    let next = reopened
        .index_record(mine("매매-2", &["새 조각 글"]))
        .unwrap();
    assert_eq!(next, vec![ChunkId(2)]);
}

// ───────────── 지식: 표 보호 ─────────────

// 검증: D10 M7
#[test]
fn 조각표에_번호가_겹치는_대체_넣기는_거부되고_색인이_그대로다() {
    let (_dir, path, store) = temp_store();
    store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    store
        .index_record(mine("매매-2", &["하이닉스 매수 신호"]))
        .unwrap();
    let before = store
        .knowledge_chunks(KnowledgeKind::Trade, "매매-1")
        .unwrap();
    let conn = raw(&path);
    for verb in ["INSERT OR REPLACE INTO", "REPLACE INTO"] {
        for key in ["id", "rowid"] {
            let sql = format!(
                "{verb} knowledge_chunks ({key}, kind, source, record_ref, ordinal, text, updated_at) \
                 SELECT id, kind, source, record_ref, ordinal, '바꿔친 글', updated_at \
                 FROM knowledge_chunks WHERE id = 1"
            );
            assert!(conn.execute(&sql, []).is_err(), "{verb} {key}가 들어갔다");
        }
    }
    drop(conn);
    assert_eq!(
        store
            .knowledge_chunks(KnowledgeKind::Trade, "매매-1")
            .unwrap(),
        before
    );
    assert_eq!(found(&store, "삼성전자"), vec!["삼성전자 손절 기록"]);
    assert_eq!(found(&store, "바꿔친"), Vec::<String>::new());
}

// 검증: D10 M7
#[test]
fn 조각표를_sql로_고치거나_지워도_검색_색인과_임베딩이_어긋나지_않는다() {
    let (_dir, path, store) = temp_store();
    let ids = store
        .index_record(mine(
            "매매-1",
            &["삼성전자 손절 기록", "하이닉스 매수 신호"],
        ))
        .unwrap();
    store.set_chunk_embedding(ids[0], "m1", &[1.0]).unwrap();
    store.set_chunk_embedding(ids[1], "m1", &[2.0]).unwrap();
    let conn = raw(&path);
    conn.execute(
        "UPDATE knowledge_chunks SET text = '네이버 실적 발표' WHERE id = 1",
        [],
    )
    .unwrap();
    assert_eq!(found(&store, "삼성전자"), Vec::<String>::new());
    assert_eq!(found(&store, "네이버"), vec!["네이버 실적 발표"]);
    conn.execute("DELETE FROM knowledge_chunks WHERE id = 2", [])
        .unwrap();
    drop(conn);
    assert_eq!(found(&store, "하이닉스"), Vec::<String>::new());
    assert_eq!(store.chunk_embedding(ids[1]).unwrap(), None);
    assert_eq!(count(&path, "SELECT count(*) FROM knowledge_embeddings"), 1);
}

// ───────────── 스냅샷 ─────────────

// 검증: D10 M7
#[test]
fn 스냅샷은_딸린_파일_없이_한_파일이고_열면_같은_자료와_버전이다() {
    let (_dir, _path, store) = temp_store();
    team(&store, TeamState::Operating, TeamCause::TradingStart, 1);
    store
        .append_at("chat", &json!({ "글": "안녕" }), t(2))
        .unwrap();
    store
        .index_record(mine("매매-1", &["삼성전자 손절 기록"]))
        .unwrap();
    store.set_knowledge_meta("model", "m1").unwrap();
    store
        .save_push_subscription(sub("https://push.example/1", 3))
        .unwrap();
    let waiting = pending(&store, "sha256:order-1");
    let out = tempfile::tempdir().unwrap();
    let copy = out.path().join("backup.sqlite");
    store.snapshot_to(&copy).unwrap();
    assert_eq!(names_in(out.path()), vec!["backup.sqlite"]);

    let opened = Store::open(&copy).unwrap();
    assert_eq!(opened.info().unwrap().schema_version, SCHEMA_VERSION);
    assert_eq!(opened.team_state().unwrap(), TeamState::Operating);
    assert_eq!(
        opened.team_history().unwrap(),
        store.team_history().unwrap()
    );
    assert_eq!(
        opened.read_after(0, 100).unwrap(),
        store.read_after(0, 100).unwrap()
    );
    assert_eq!(found(&opened, "삼성전자"), vec!["삼성전자 손절 기록"]);
    assert_eq!(
        opened.knowledge_meta("model").unwrap().as_deref(),
        Some("m1")
    );
    assert_eq!(
        opened.push_subscriptions().unwrap(),
        store.push_subscriptions().unwrap()
    );
    assert_eq!(
        opened.get_approval(waiting).unwrap().status,
        ApprovalStatus::Pending
    );
    drop(opened);
    assert_eq!(names_in(out.path()), vec!["backup.sqlite"]);
}

// 검증: D10 M7
#[test]
fn 스냅샷_뒤에_원본에_쓴_것은_복사본에_없고_원본은_그대로_쓸_수_있다() {
    let (_dir, _path, store) = temp_store();
    store.append("chat", &json!(1)).unwrap();
    let out = tempfile::tempdir().unwrap();
    let copy = out.path().join("backup.sqlite");
    store.snapshot_to(&copy).unwrap();
    store.append("chat", &json!(2)).unwrap();
    assert_eq!(store.read_after(0, 10).unwrap().len(), 2);
    assert_eq!(
        Store::open(&copy).unwrap().read_after(0, 10).unwrap().len(),
        1
    );
}

// 검증: D10 M7
#[test]
fn 쓰는_중에_떠도_한_시점의_이어진_번호만_들어_있다() {
    let (_dir, _path, store) = temp_store();
    store.append("chat", &json!(0)).unwrap();
    let writer = {
        let store = store.clone();
        std::thread::spawn(move || {
            for n in 1..=150 {
                store.append("chat", &json!(n)).unwrap();
            }
        })
    };
    let out = tempfile::tempdir().unwrap();
    let copy = out.path().join("backup.sqlite");
    store.snapshot_to(&copy).unwrap();
    writer.join().unwrap();
    let events = Store::open(&copy).unwrap().read_after(0, 1000).unwrap();
    assert!(!events.is_empty());
    for (i, event) in events.iter().enumerate() {
        assert_eq!(event.seq, u64::try_from(i).unwrap() + 1);
    }
}

// 검증: D10 M7
#[cfg(unix)]
#[test]
fn 스냅샷_파일의_권한은_0600이다() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, _path, store) = temp_store();
    let out = tempfile::tempdir().unwrap();
    let copy = out.path().join("backup.sqlite");
    store.snapshot_to(&copy).unwrap();
    let mode = fs::metadata(&copy).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

// 검증: D10 M7
#[test]
fn 이미_파일이_있는_자리에는_뜨지_않고_그_파일이_그대로다() {
    let (_dir, path, store) = temp_store();
    store.append("chat", &json!(1)).unwrap();
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("taken.sqlite");
    fs::write(&target, b"original bytes").unwrap();
    let err = store.snapshot_to(&target).unwrap_err();
    assert!(
        matches!(err, StoreError::SnapshotTargetExists(ref p) if p == &target),
        "{err:?}"
    );
    assert_eq!(fs::read(&target).unwrap(), b"original bytes");
    let err = store.snapshot_to(&path).unwrap_err();
    assert!(
        matches!(err, StoreError::SnapshotTargetExists(_)),
        "{err:?}"
    );
    assert_eq!(store.read_after(0, 10).unwrap().len(), 1);
}

// 검증: D10 M7
#[test]
fn 없는_폴더로_뜨면_오류이고_파일이_남지_않는다() {
    let (_dir, _path, store) = temp_store();
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("없는폴더").join("backup.sqlite");
    assert!(store.snapshot_to(&target).is_err());
    assert!(names_in(out.path()).is_empty());
    assert_eq!(store.team_state().unwrap(), TeamState::Resting);
}

// ───────────── 되살리기 표시 (V51) ─────────────

// 검증: V51
#[test]
fn 되살리면_팀이_전체_정지가_되고_기다리던_승인이_만료된다() {
    let (_d1, original, _d2, copy, waiting) = snapped();
    let restored = Store::open(&copy).unwrap();
    assert_eq!(restored.team_state().unwrap(), TeamState::Operating);
    let before = restored.team_history().unwrap();
    let outcome = restored.mark_restored(t(500)).unwrap();
    assert_eq!(outcome.expired_approvals, vec![waiting]);
    let record = outcome
        .team_record
        .expect("전체 정지로 바뀌며 기록이 남는다");
    assert_eq!(record.seq, before.last().unwrap().seq + 1);
    assert_eq!(record.at, t(500));
    assert_eq!(record.from, TeamState::Operating);
    assert_eq!(record.to, TeamState::FullStop);
    assert_eq!(record.cause, TeamCause::Restored);
    assert!(!record.owner_approved);
    assert_eq!(restored.team_state().unwrap(), TeamState::FullStop);
    assert_eq!(restored.team_history().unwrap().last(), Some(&record));
    let approval = restored.get_approval(waiting).unwrap();
    assert_eq!(approval.status, ApprovalStatus::Expired);
    assert_eq!(approval.resolved_at, Some(t(500)));
    assert_eq!(approval.consumed_at, None);
    // 원본은 그대로다.
    assert_eq!(original.team_state().unwrap(), TeamState::Operating);
    assert_eq!(
        original.get_approval(waiting).unwrap().status,
        ApprovalStatus::Pending
    );
}

// 검증: V51
#[test]
fn 만료된_승인은_뒤늦게_승인해도_되살아나지_않는다() {
    let (_d1, _original, _d2, copy, waiting) = snapped();
    let restored = Store::open(&copy).unwrap();
    restored.mark_restored(t(500)).unwrap();
    let err = restored
        .resolve_approval(waiting, ApprovalStatus::Approved, t(501))
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::TransitionDenied {
                transition: Transition::Approval { .. },
                reason: DenyReason::NotAllowed
            }
        ),
        "{err:?}"
    );
    assert_eq!(
        restored.get_approval(waiting).unwrap().status,
        ApprovalStatus::Expired
    );
}

// 검증: V51
#[test]
fn 이미_답이_난_승인은_건드리지_않고_기다림만_만료한다() {
    let (_dir, _path, store) = temp_store();
    let approved = pending(&store, "sha256:a");
    let rejected = pending(&store, "sha256:b");
    let no_answer = pending(&store, "sha256:c");
    let expired = store
        .create_approval(NewApprovalRequest {
            fingerprint: "sha256:d".to_string(),
            summary: json!({}),
            created_at: t(0),
            expires_at: t(10),
        })
        .unwrap()
        .id;
    let waiting_a = pending(&store, "sha256:e");
    let waiting_b = pending(&store, "sha256:f");
    store
        .resolve_approval(approved, ApprovalStatus::Approved, t(1))
        .unwrap();
    store
        .resolve_approval(rejected, ApprovalStatus::Rejected, t(2))
        .unwrap();
    store
        .resolve_approval(no_answer, ApprovalStatus::NoAnswer, t(3))
        .unwrap();
    store
        .resolve_approval(expired, ApprovalStatus::Approved, t(20))
        .unwrap();
    let before: Vec<_> = [approved, rejected, no_answer, expired]
        .iter()
        .map(|id| store.get_approval(*id).unwrap())
        .collect();
    let outcome = store.mark_restored(t(500)).unwrap();
    assert_eq!(outcome.expired_approvals, vec![waiting_a, waiting_b]);
    let after: Vec<_> = [approved, rejected, no_answer, expired]
        .iter()
        .map(|id| store.get_approval(*id).unwrap())
        .collect();
    assert_eq!(after, before);
    assert_eq!(
        store.get_approval(waiting_b).unwrap().status,
        ApprovalStatus::Expired
    );
}

// 검증: V51
#[test]
fn 쉬는_중에서도_전체_정지가_되고_승인이_없으면_만료할_것이_없다() {
    let (_dir, _path, store) = temp_store();
    let outcome = store.mark_restored(t(7)).unwrap();
    assert!(outcome.expired_approvals.is_empty());
    let record = outcome.team_record.unwrap();
    assert_eq!(
        (record.from, record.to, record.cause),
        (TeamState::Resting, TeamState::FullStop, TeamCause::Restored)
    );
    assert_eq!(store.team_state().unwrap(), TeamState::FullStop);
}

// 검증: V51
#[test]
fn 이미_전체_정지이면_기록을_더하지_않고_승인은_만료하고_다시_불러도_같다() {
    let (_dir, _path, store) = temp_store();
    team(&store, TeamState::Operating, TeamCause::TradingStart, 1);
    team(&store, TeamState::FullStop, TeamCause::MaxLoss, 2);
    let waiting = pending(&store, "sha256:g");
    let history = store.team_history().unwrap();
    let outcome = store.mark_restored(t(9)).unwrap();
    assert_eq!(outcome.team_record, None);
    assert_eq!(outcome.expired_approvals, vec![waiting]);
    assert_eq!(store.team_history().unwrap(), history);
    let again = store.mark_restored(t(10)).unwrap();
    assert_eq!(again.team_record, None);
    assert!(again.expired_approvals.is_empty());
    assert_eq!(store.team_history().unwrap(), history);
    assert_eq!(store.get_approval(waiting).unwrap().resolved_at, Some(t(9)));
}

// 검증: V51 M7
#[test]
fn 되살리면_실전중_템플릿은_멈춤이_된다() {
    let (_dir, _path, store) = temp_store();
    team(&store, TeamState::Operating, TeamCause::TradingStart, 30);
    let template = live_template(&store);
    assert_eq!(template.state, TemplateState::Live);
    store.mark_restored(t(500)).unwrap();
    assert_eq!(
        store.get_template(template.id).unwrap().state,
        TemplateState::Stopped
    );
}

// 검증: V51
#[test]
fn 되살린_뒤_전체_정지에서_나가려면_내_승인이_있어야_하고_그_사유로_가는_다른_길은_없다() {
    let (_dir, _path, store) = temp_store();
    team(&store, TeamState::Operating, TeamCause::TradingStart, 1);
    let change = |to, cause, approved| TeamChange {
        to,
        at: t(600),
        cause,
        detail: "시험".to_string(),
        owner_approved: approved,
    };
    // 되살리기 사유로 전체 정지에 가는 길은 mark_restored 하나다.
    let err = store
        .change_team_state(change(TeamState::FullStop, TeamCause::Restored, false))
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::TransitionDenied {
                reason: DenyReason::MaxLossOnly,
                ..
            }
        ),
        "{err:?}"
    );
    store.mark_restored(t(500)).unwrap();
    let err = store
        .change_team_state(change(TeamState::Resting, TeamCause::OwnerRestart, false))
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::TransitionDenied {
                reason: DenyReason::OwnerApprovalRequired,
                ..
            }
        ),
        "{err:?}"
    );
    assert_eq!(store.team_state().unwrap(), TeamState::FullStop);
    store
        .change_team_state(change(TeamState::Resting, TeamCause::OwnerRestart, true))
        .unwrap();
    assert_eq!(store.team_state().unwrap(), TeamState::Resting);
}

// 검증: V51
#[test]
fn 되살리기_표시는_다시_열어도_그대로이고_복사본_파일에_남는다() {
    let (_d1, _original, _d2, copy, waiting) = snapped();
    let restored = Store::open(&copy).unwrap();
    restored.mark_restored(t(500)).unwrap();
    drop(restored);
    let reopened = Store::open(&copy).unwrap();
    assert_eq!(reopened.team_state().unwrap(), TeamState::FullStop);
    assert_eq!(
        reopened.get_approval(waiting).unwrap().status,
        ApprovalStatus::Expired
    );
    assert_eq!(
        reopened.team_history().unwrap().last().unwrap().cause,
        TeamCause::Restored
    );
}

// ───────────── 웹 푸시 구독 ─────────────

// 검증: D10
#[test]
fn 구독을_두면_값이_그대로_나오고_처음_둔_순서로_읽힌다() {
    let (_dir, _path, store) = temp_store();
    let first = store
        .save_push_subscription(sub("https://push.example/b", 5))
        .unwrap();
    store
        .save_push_subscription(sub("https://push.example/a", 6))
        .unwrap();
    assert_eq!(first.endpoint, "https://push.example/b");
    assert_eq!(first.p256dh, "공개키-시험");
    assert_eq!(first.auth, "인증-시험");
    assert_eq!(first.device, "폰");
    assert_eq!((first.created_at, first.updated_at), (t(5), t(5)));
    let all = store.push_subscriptions().unwrap();
    let endpoints: Vec<&str> = all.iter().map(|s| s.endpoint.as_str()).collect();
    assert_eq!(
        endpoints,
        vec!["https://push.example/b", "https://push.example/a"]
    );
}

// 검증: D10
#[test]
fn 같은_주소를_다시_두면_키와_이름표만_바뀌고_줄은_늘지_않는다() {
    let (_dir, _path, store) = temp_store();
    store
        .save_push_subscription(sub("https://push.example/1", 5))
        .unwrap();
    let mut changed = sub("https://push.example/1", 50);
    changed.p256dh = "새-공개키".to_string();
    changed.auth = "새-인증".to_string();
    changed.device = "태블릿".to_string();
    let saved = store.save_push_subscription(changed).unwrap();
    assert_eq!((saved.created_at, saved.updated_at), (t(5), t(50)));
    let all = store.push_subscriptions().unwrap();
    assert_eq!(all, vec![saved]);
    assert_eq!(
        (
            all[0].p256dh.as_str(),
            all[0].auth.as_str(),
            all[0].device.as_str()
        ),
        ("새-공개키", "새-인증", "태블릿")
    );
}

// 검증: D10
#[test]
fn 규칙에_어긋난_구독은_거부하고_아무것도_두지_않는다() {
    let (_dir, _path, store) = temp_store();
    let http = sub("http://push.example/1", 1);
    let mut no_key = sub("https://push.example/2", 1);
    no_key.p256dh = String::new();
    let mut no_auth = sub("https://push.example/3", 1);
    no_auth.auth = String::new();
    for bad in [http, no_key, no_auth, sub("", 1), sub("push.example/4", 1)] {
        let err = store.save_push_subscription(bad).unwrap_err();
        assert!(
            matches!(err, StoreError::InvalidPushSubscription),
            "{err:?}"
        );
    }
    assert!(store.push_subscriptions().unwrap().is_empty());
}

// 검증: D10
#[test]
fn 구독을_지우면_참이고_없는_구독은_거짓이며_다시_열어도_그대로다() {
    let (_dir, path, store) = temp_store();
    store
        .save_push_subscription(sub("https://push.example/1", 1))
        .unwrap();
    store
        .save_push_subscription(sub("https://push.example/2", 2))
        .unwrap();
    assert!(
        store
            .remove_push_subscription("https://push.example/1")
            .unwrap()
    );
    assert!(
        !store
            .remove_push_subscription("https://push.example/1")
            .unwrap()
    );
    assert!(
        !store
            .remove_push_subscription("https://push.example/없음")
            .unwrap()
    );
    drop(store);
    let reopened = Store::open(&path).unwrap();
    let left = reopened.push_subscriptions().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].endpoint, "https://push.example/2");
}
