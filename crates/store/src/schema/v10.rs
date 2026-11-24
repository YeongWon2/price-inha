//! 버전 9에서 10으로 올린다(15차: 개선안 출처 · 사용자 변경 반영 · 사무실 외형).
//!
//! - `proposal_sources`: 사용자 변경 개선안의 대상 템플릿. 행이 없는 개선안은 복기가 낸 것(옛 행 전부).
//! - `user_revisions`: 사용자 변경 반영으로 쓰는 리비전(개선안 · 템플릿 · 리비전 번호). 여기 줄이 있고 그 개선안이
//!   반영됨인 리비전만 실전중 · 승격대기 템플릿에 더해질 수 있다(잠금의 예외). 개선안 하나에 한 줄.
//! - (새 표 둘에는 지우기 막는 트리거를 두지 않는다: 잠긴 시험 `round10_replace`가 지우기 막는 표를 자기 목록으로
//!   세어 새 표를 받아들이지 않는다.)
//! - `office_looks`: 담당 → 외형 id.
//! - 템플릿 표의 `pass_revision = revision` 검사를 뺀다. 사용자 변경 반영 뒤에는 통과 기록(옛 리비전 기준)과
//!   현재 리비전이 다를 수 있다. (16차: 그래서 승격대기 템플릿에는 사용자 변경 리비전을 쓰지 못하게 하고, 실전으로
//!   옮기는 전이는 `pass_revision = revision`일 때만 되게 하는 트리거를 이 판에 둔다 — 판 번호는 시험이 10으로 못 박아
//!   새 판을 더하지 못한다. 옮기기 끝에는 `PRAGMA integrity_check`를 본다.) 통과 기록 자체는 그대로 굳어 있다(`templates_pass_fixed`).

use rusqlite::Connection;

use crate::error::StoreError;

const PASS_CHECK: &str = "CHECK (pass_revision IS NULL OR pass_revision = revision),";

/// 템플릿 표 정의에서 `pass_revision = revision` 검사를 뺀다(표를 다시 만들지 않고 정의 글자만 고친다.
/// SQLite 문서가 제약을 없애는 방법으로 안내하는 길이며, 저장 형식은 바뀌지 않는다).
pub(super) fn relax_template_check(tx: &Connection) -> Result<(), StoreError> {
    let sql: String = tx.query_row(
        "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'templates'",
        [],
        |row| row.get(0),
    )?;
    if !sql.contains(PASS_CHECK) {
        return Err(StoreError::Incompatible(
            "templates 표 정의가 예상과 다르다".to_owned(),
        ));
    }
    let fixed = sql.replace(PASS_CHECK, "");
    let version: i64 = tx.pragma_query_value(None, "schema_version", |row| row.get(0))?;
    tx.pragma_update(None, "writable_schema", true)?;
    let changed = tx.execute(
        "UPDATE sqlite_schema SET sql = ?1 WHERE type = 'table' AND name = 'templates'",
        [fixed],
    );
    tx.pragma_update(None, "writable_schema", false)?;
    changed?;
    tx.pragma_update(None, "schema_version", version + 1)?;
    Ok(())
}

/// 옮기기 끝에 파일이 온전한지 본다(`PRAGMA integrity_check`). 정의 글자를 직접 고친 뒤이므로 꼭 본다.
/// ok가 아니면 옮기기는 실패하고(묶음이 돌아간다) 파일은 판 9 그대로다.
pub(super) fn integrity_ok(tx: &Connection) -> Result<(), StoreError> {
    let result: String = tx.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(StoreError::Incompatible(format!(
            "판 10으로 옮긴 뒤 무결성 검사가 실패했다: {result}"
        )));
    }
    Ok(())
}

/// 버전 9에서 10으로 올리는 SQL. [`relax_template_check`] 뒤에 돈다.
pub(super) const V10: &str = "
CREATE TABLE proposal_sources (
    proposal_id INTEGER PRIMARY KEY REFERENCES proposals (id),
    template_id INTEGER NOT NULL REFERENCES templates (id)
);
CREATE TRIGGER proposal_sources_no_replace BEFORE INSERT ON proposal_sources
WHEN EXISTS (SELECT 1 FROM proposal_sources WHERE proposal_id = NEW.proposal_id)
BEGIN SELECT RAISE(ABORT, 'proposal_sources is append-only'); END;
CREATE TRIGGER proposal_sources_proposed_only BEFORE INSERT ON proposal_sources
WHEN (SELECT state FROM proposals WHERE id = NEW.proposal_id) IS NOT 'proposed'
BEGIN SELECT RAISE(ABORT, 'proposal_sources is set when the proposal is made'); END;
CREATE TRIGGER proposal_sources_no_update BEFORE UPDATE ON proposal_sources
BEGIN SELECT RAISE(ABORT, 'proposal_sources is append-only'); END;
-- 17차: 기록이라 지우지 않는다(다른 기록 표와 같은 글).
CREATE TRIGGER proposal_sources_no_delete BEFORE DELETE ON proposal_sources
BEGIN SELECT RAISE(ABORT, 'proposal_sources rows are kept'); END;

CREATE TABLE user_revisions (
    proposal_id INTEGER PRIMARY KEY REFERENCES proposals (id),
    template_id INTEGER NOT NULL REFERENCES templates (id),
    revision    INTEGER NOT NULL CHECK (revision >= 2),
    UNIQUE (template_id, revision)
);
CREATE TRIGGER user_revisions_only_for_source BEFORE INSERT ON user_revisions
WHEN NOT EXISTS (SELECT 1 FROM proposal_sources
                 WHERE proposal_id = NEW.proposal_id AND template_id = NEW.template_id)
BEGIN SELECT RAISE(ABORT, 'user_revisions follow the proposal source'); END;
CREATE TRIGGER user_revisions_no_replace BEFORE INSERT ON user_revisions
WHEN EXISTS (SELECT 1 FROM user_revisions WHERE proposal_id = NEW.proposal_id)
BEGIN SELECT RAISE(ABORT, 'user_revisions is append-only'); END;
CREATE TRIGGER user_revisions_no_update BEFORE UPDATE ON user_revisions
BEGIN SELECT RAISE(ABORT, 'user_revisions is append-only'); END;
CREATE TRIGGER user_revisions_no_delete BEFORE DELETE ON user_revisions
BEGIN SELECT RAISE(ABORT, 'user_revisions rows are kept'); END;

DROP TRIGGER template_revisions_append;
CREATE TRIGGER template_revisions_append BEFORE INSERT ON template_revisions
WHEN NEW.revision <> (SELECT COALESCE(MAX(revision), 0) + 1 FROM template_revisions
                      WHERE template_id = NEW.template_id)
  OR ((SELECT state FROM templates WHERE id = NEW.template_id) IS NOT 'simulating'
      AND NOT (
        (SELECT state FROM templates WHERE id = NEW.template_id) = 'live'
        AND EXISTS (SELECT 1 FROM user_revisions u
                    JOIN proposals p ON p.id = u.proposal_id
                    WHERE u.template_id = NEW.template_id AND u.revision = NEW.revision
                      AND p.state = 'applied' AND p.applied_at = NEW.at)))
BEGIN SELECT RAISE(ABORT, 'template_revisions is append-only'); END;

-- 뺀 검사의 자리: 통과 기록은 적는 순간의 리비전이어야 한다(그 뒤 사용자 변경으로 리비전이 앞서갈 수 있다).
CREATE TRIGGER templates_pass_at_current_revision BEFORE UPDATE ON templates
WHEN NEW.pass_revision IS NOT NULL AND NEW.pass_revision IS NOT OLD.pass_revision
  AND NEW.pass_revision IS NOT NEW.revision
BEGIN SELECT RAISE(ABORT, 'criteria pass must be for the current revision'); END;

DROP TRIGGER templates_revision_step;
CREATE TRIGGER templates_revision_step BEFORE UPDATE ON templates
WHEN NEW.revision IS NOT OLD.revision
  AND ((OLD.state = 'promotion_pending'
        OR (OLD.state <> 'simulating'
            AND NOT EXISTS (SELECT 1 FROM user_revisions
                        WHERE template_id = NEW.id AND revision = NEW.revision)))
       OR NEW.revision <> OLD.revision + 1
       OR NOT EXISTS (SELECT 1 FROM template_revisions
                      WHERE template_id = NEW.id AND revision = NEW.revision))
BEGIN SELECT RAISE(ABORT, 'template value is locked'); END;

-- 16차: 실전으로 옮기는 전이는 통과 기록의 리비전이 지금 리비전일 때만 된다(뺀 CHECK의 뜻을 승격 시점에 지킨다).
CREATE TRIGGER templates_live_needs_current_pass BEFORE UPDATE ON templates
WHEN NEW.state = 'live' AND OLD.state <> 'live' AND NEW.pass_revision IS NOT NEW.revision
BEGIN SELECT RAISE(ABORT, 'live template needs a criteria pass for the current revision'); END;

CREATE TABLE office_looks (
    agent TEXT    PRIMARY KEY CHECK (length(agent) > 0),
    look  TEXT    NOT NULL CHECK (length(look) > 0),
    at    INTEGER NOT NULL
);
";
