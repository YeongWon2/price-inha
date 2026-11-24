#![cfg(test)]
//! 17차: 새 기록 표(`proposal_sources` · `user_revisions`)의 줄은 직접 SQL로도 지워지지 않고,
//! 고쳐지지 않고, 같은 줄을 다시 넣는 우회도 거부된다. 사용자 변경 개선안 흐름(15차)은 그대로다.

mod common;

use std::path::Path;

use common::{t, temp_store};
use pi_store::{
    ApprovalStatus, ApprovalUse, NewApprovalRequest, NewTemplate, NewUserProposal, Proposal,
    ProposalSource, ProposalState, Store, Template, TemplateState,
};
use rusqlite::types::Value as SqlValue;
use serde_json::json;

const TABLES: [&str; 2] = ["proposal_sources", "user_revisions"];

fn new_value() -> serde_json::Value {
    json!({ "스타일": "스윙", "R%": "1.5" })
}

fn template(store: &Store) -> Template {
    store
        .create_template(NewTemplate {
            value: json!({ "스타일": "스윙", "R%": "1.0" }),
            at: t(0),
        })
        .unwrap()
}

fn cooling_user_proposal(store: &Store, template: &Template) -> Proposal {
    let made = store
        .create_user_proposal(NewUserProposal {
            template: template.id,
            content: new_value(),
            at: t(30),
        })
        .unwrap();
    let id = store
        .create_approval(NewApprovalRequest {
            fingerprint: "sha256:round17".to_string(),
            summary: json!({ "대상": "round17" }),
            created_at: t(0),
            expires_at: t(1_000_000),
        })
        .unwrap()
        .id;
    store
        .resolve_approval(id, ApprovalStatus::Approved, t(1))
        .unwrap();
    let cooling = store
        .approve_proposal(
            made.id,
            ApprovalUse {
                id,
                fingerprint: "sha256:round17".to_string(),
            },
            t(31),
        )
        .unwrap();
    assert_eq!(cooling.state, ProposalState::Cooling);
    cooling
}

/// 두 표에 줄이 하나씩 생긴 파일(사용자 변경 개선안을 만들고 반영했다).
fn filled() -> (tempfile::TempDir, std::path::PathBuf, Proposal) {
    let (dir, path, store) = temp_store();
    let made = template(&store);
    let cooling = cooling_user_proposal(&store, &made);
    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();
    drop(store);
    (dir, path, cooling)
}

/// 우회하는 쪽의 연결(외래 키 검사를 끈다 — 표 스스로 막아야 한다).
fn raw(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn
}

fn rows(path: &Path, table: &str) -> Vec<Vec<SqlValue>> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let mut stmt = conn
        .prepare(&format!("SELECT rowid, * FROM \"{table}\" ORDER BY rowid"))
        .unwrap();
    let width = stmt.column_count();
    stmt.query_map([], |row| {
        (0..width)
            .map(|i| row.get::<_, SqlValue>(i))
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn columns(path: &Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).unwrap();
    let cols: Vec<String> = conn
        .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(!cols.is_empty(), "{table} 표가 없다");
    cols
}

/// 제약 오류로 거부되면 그 글을, 들어가 버리면 `None`. 그 밖의 오류는 시험 SQL이 틀린 것이다.
fn rejection(path: &Path, sql: &str) -> Option<String> {
    match raw(path).execute(sql, []) {
        Ok(changed) => {
            assert!(changed > 0, "아무 줄도 고르지 못한 SQL이다: {sql}");
            None
        }
        Err(rusqlite::Error::SqliteFailure(e, message))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Some(message.unwrap_or_default())
        }
        Err(other) => panic!("시험하는 SQL이 틀렸다: {sql}\n{other}"),
    }
}

// 검증: M1 M2 C11 V36
#[test]
fn 두_표에는_줄이_있고_직접_지우기는_거부되며_줄이_그대로다() {
    let (_dir, path, _p) = filled();
    for table in TABLES {
        let before = rows(&path, table);
        assert!(!before.is_empty(), "{table}에 줄이 없다");
        let message = rejection(&path, &format!("DELETE FROM \"{table}\""));
        let message = message.unwrap_or_else(|| panic!("{table}: 지우기가 들어갔다"));
        assert!(
            message.contains("rows are kept"),
            "{table}: 다른 기록 표와 같은 글이 아니다: {message}"
        );
        assert_eq!(rows(&path, table), before, "{table}");
    }
}

// 검증: M1 M2 C11 V36
#[test]
fn 줄을_골라_지워도_거부되고_줄이_그대로다() {
    let (_dir, path, made) = filled();
    for table in TABLES {
        let before = rows(&path, table);
        let sql = format!("DELETE FROM \"{table}\" WHERE proposal_id = {}", made.id.0);
        assert!(
            rejection(&path, &sql).is_some(),
            "{table}: 지우기가 들어갔다"
        );
        assert_eq!(rows(&path, table), before, "{table}");
    }
}

// 검증: M1 M2 C11 V36
#[test]
fn 두_표의_줄_고치기는_거부되고_줄이_그대로다() {
    let (_dir, path, _p) = filled();
    for (table, sql) in [
        (
            "proposal_sources",
            "UPDATE proposal_sources SET template_id = template_id + 1",
        ),
        (
            "user_revisions",
            "UPDATE user_revisions SET revision = revision + 1",
        ),
        (
            "user_revisions",
            "UPDATE user_revisions SET template_id = template_id + 1",
        ),
    ] {
        let before = rows(&path, table);
        assert!(rejection(&path, sql).is_some(), "고치기가 들어갔다: {sql}");
        assert_eq!(rows(&path, table), before, "{sql}");
    }
}

// 검증: M1 M2 C11 V36
#[test]
fn 같은_줄을_다시_넣는_우회는_두_표_모두_거부되고_줄이_그대로다() {
    let (_dir, path, _p) = filled();
    for table in TABLES {
        let before = rows(&path, table);
        let cols = columns(&path, table).join(", ");
        for verb in ["INSERT OR REPLACE INTO", "REPLACE INTO"] {
            let sql = format!("{verb} \"{table}\" ({cols}) SELECT {cols} FROM \"{table}\"");
            assert!(
                rejection(&path, &sql).is_some(),
                "{table}: 바꿔치기가 들어갔다: {sql}"
            );
            assert_eq!(rows(&path, table), before, "{sql}");
        }
    }
}

// 검증: M1 M2 C11 V36
#[test]
fn 막힌_뒤에도_공개_api로_읽은_값이_그대로다() {
    let (_dir, path, made) = filled();
    for table in TABLES {
        let _ = rejection(&path, &format!("DELETE FROM \"{table}\""));
    }
    let store = Store::open(&path).unwrap();
    let proposal = store.get_proposal(made.id).unwrap();
    assert_eq!(proposal.state, ProposalState::Applied);
    assert!(matches!(
        store.proposal_source(made.id).unwrap(),
        ProposalSource::User { .. }
    ));
    let revisions = store
        .template_revisions(match store.proposal_source(made.id).unwrap() {
            ProposalSource::User { template } => template,
            ProposalSource::Review => panic!("사용자 변경 개선안이어야 한다"),
        })
        .unwrap();
    assert_eq!(revisions.len(), 2);
}

// 검증: R14-1 E28 V15
#[test]
fn 사용자_변경_개선안_흐름은_그대로_되고_다시_열어도_같다() {
    let (dir, path, store) = temp_store();
    let made = template(&store);
    let cooling = cooling_user_proposal(&store, &made);
    store
        .apply_cooled_user_proposal(cooling.id, &new_value(), t(50))
        .unwrap();

    let applied = store.get_proposal(cooling.id).unwrap();
    assert_eq!(applied.state, ProposalState::Applied);
    let now = store.get_template(made.id).unwrap();
    assert_eq!(now.state, TemplateState::Simulating);
    assert_eq!(now.revision, 2);
    assert_eq!(now.value, new_value());
    assert_eq!(
        store.proposal_source(cooling.id).unwrap(),
        ProposalSource::User { template: made.id }
    );
    drop(store);

    let reopened = Store::open(dir.path().join("pi.sqlite")).unwrap();
    assert_eq!(reopened.get_proposal(cooling.id).unwrap(), applied);
    assert_eq!(reopened.get_template(made.id).unwrap(), now);
    assert_eq!(rows(&path, "proposal_sources").len(), 1);
    assert_eq!(rows(&path, "user_revisions").len(), 1);
}
