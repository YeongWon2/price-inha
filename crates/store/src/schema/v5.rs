//! 버전 4 → 5 (9차): 템플릿 · 승인 · 규칙 개선안 · 모의계좌 상태 값 · 접속 키 해시 표.
//!
//! 표가 스스로 지키는 것(어떤 SQL로도):
//! - 상태 칸은 CHECK가 아는 글자만 받는다. 상태와 함께 있어야 하는 칸(승인 번호 · 기준 통과 기록 ·
//!   답한 시각 · 소비 시각 · 반영 시각)의 짝도 CHECK가 본다.
//! - 줄은 지워지지 않는다(DELETE · INSERT OR REPLACE 모두 막는다).
//! - 상태는 상태표에 있는 전이로만 바뀌고, 한 번 정해진 칸(내용 · 지문 · 만든 시각 · 승인 번호 ·
//!   소비 시각)은 되돌리거나 고칠 수 없다.
//! - 판 · 상태 값 · 접속 키 · 근거 매매처럼 쌓기만 하는 표는 고치기 · 지우기를 막는다.

/// 버전 4에서 5로 올리는 SQL.
pub(super) const V5: &str = "
CREATE TABLE approvals (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    fingerprint TEXT    NOT NULL,
    summary     TEXT    NOT NULL,
    status      TEXT    NOT NULL
        CHECK (status IN ('pending', 'approved', 'rejected', 'no_answer', 'expired')),
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    resolved_at INTEGER,
    consumed_at INTEGER,
    CHECK ((status = 'pending') = (resolved_at IS NULL)),
    CHECK (consumed_at IS NULL OR status = 'approved')
);
CREATE INDEX approvals_by_status ON approvals (status, id);
CREATE TRIGGER approvals_start_pending BEFORE INSERT ON approvals
WHEN NEW.status <> 'pending' OR NEW.consumed_at IS NOT NULL
BEGIN SELECT RAISE(ABORT, 'approvals start pending'); END;
CREATE TRIGGER approvals_no_replace BEFORE INSERT ON approvals
WHEN EXISTS (SELECT 1 FROM approvals WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'approvals rows are kept'); END;
CREATE TRIGGER approvals_no_delete BEFORE DELETE ON approvals
BEGIN SELECT RAISE(ABORT, 'approvals rows are kept'); END;
CREATE TRIGGER approvals_fixed BEFORE UPDATE ON approvals
WHEN NEW.id IS NOT OLD.id OR NEW.fingerprint IS NOT OLD.fingerprint
  OR NEW.summary IS NOT OLD.summary OR NEW.created_at IS NOT OLD.created_at
  OR NEW.expires_at IS NOT OLD.expires_at
BEGIN SELECT RAISE(ABORT, 'approval request is fixed'); END;
CREATE TRIGGER approvals_resolve_once BEFORE UPDATE ON approvals
WHEN OLD.status <> 'pending'
  AND (NEW.status IS NOT OLD.status OR NEW.resolved_at IS NOT OLD.resolved_at)
BEGIN SELECT RAISE(ABORT, 'approval is resolved once'); END;
CREATE TRIGGER approvals_consume_once BEFORE UPDATE ON approvals
WHEN OLD.consumed_at IS NOT NULL AND NEW.consumed_at IS NOT OLD.consumed_at
BEGIN SELECT RAISE(ABORT, 'approval is consumed once'); END;

CREATE TABLE templates (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    state         TEXT    NOT NULL
        CHECK (state IN ('simulating', 'promotion_pending', 'live', 'stopped')),
    revision      INTEGER NOT NULL CHECK (revision >= 1),
    pass_revision INTEGER,
    pass_record   TEXT,
    pass_at       INTEGER,
    approval_id   INTEGER REFERENCES approvals (id),
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    CHECK ((pass_revision IS NULL) = (pass_record IS NULL)
       AND (pass_record IS NULL) = (pass_at IS NULL)),
    CHECK ((state = 'simulating') = (pass_record IS NULL)),
    CHECK ((state IN ('live', 'stopped')) = (approval_id IS NOT NULL))
);
CREATE INDEX templates_by_state ON templates (state, id);
CREATE TABLE template_revisions (
    template_id INTEGER NOT NULL REFERENCES templates (id),
    revision    INTEGER NOT NULL CHECK (revision >= 1),
    value       TEXT    NOT NULL,
    at          INTEGER NOT NULL,
    PRIMARY KEY (template_id, revision)
);
CREATE TRIGGER templates_start_simulating BEFORE INSERT ON templates
WHEN NEW.state <> 'simulating' OR NEW.revision <> 1
BEGIN SELECT RAISE(ABORT, 'templates start simulating at revision 1'); END;
CREATE TRIGGER templates_no_replace BEFORE INSERT ON templates
WHEN EXISTS (SELECT 1 FROM templates WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'templates rows are kept'); END;
CREATE TRIGGER templates_no_delete BEFORE DELETE ON templates
BEGIN SELECT RAISE(ABORT, 'templates rows are kept'); END;
CREATE TRIGGER templates_fixed BEFORE UPDATE ON templates
WHEN NEW.id IS NOT OLD.id OR NEW.created_at IS NOT OLD.created_at
  OR (OLD.approval_id IS NOT NULL AND NEW.approval_id IS NOT OLD.approval_id)
BEGIN SELECT RAISE(ABORT, 'template is fixed'); END;
CREATE TRIGGER templates_transition BEFORE UPDATE ON templates
WHEN NEW.state IS NOT OLD.state AND NOT (
     (OLD.state = 'simulating' AND NEW.state = 'promotion_pending')
  OR (OLD.state = 'promotion_pending' AND NEW.state IN ('simulating', 'live'))
  OR (OLD.state = 'live' AND NEW.state = 'stopped'))
BEGIN SELECT RAISE(ABORT, 'template transition is not allowed'); END;
CREATE TRIGGER templates_live_needs_consumed_approval BEFORE UPDATE ON templates
WHEN NEW.state = 'live' AND OLD.state <> 'live' AND NOT EXISTS (
  SELECT 1 FROM approvals
  WHERE id = NEW.approval_id AND status = 'approved' AND consumed_at IS NOT NULL)
BEGIN SELECT RAISE(ABORT, 'live template needs a consumed approval'); END;
CREATE TRIGGER templates_revision_step BEFORE UPDATE ON templates
WHEN NEW.revision IS NOT OLD.revision
  AND (OLD.state <> 'simulating' OR NEW.revision <> OLD.revision + 1
       OR NOT EXISTS (SELECT 1 FROM template_revisions
                      WHERE template_id = NEW.id AND revision = NEW.revision))
BEGIN SELECT RAISE(ABORT, 'template value is locked'); END;
CREATE TRIGGER template_revisions_append BEFORE INSERT ON template_revisions
WHEN NEW.revision <> (SELECT COALESCE(MAX(revision), 0) + 1 FROM template_revisions
                      WHERE template_id = NEW.template_id)
  OR (SELECT state FROM templates WHERE id = NEW.template_id) IS NOT 'simulating'
BEGIN SELECT RAISE(ABORT, 'template_revisions is append-only'); END;
CREATE TRIGGER template_revisions_no_update BEFORE UPDATE ON template_revisions
BEGIN SELECT RAISE(ABORT, 'template_revisions is append-only'); END;
CREATE TRIGGER template_revisions_no_delete BEFORE DELETE ON template_revisions
BEGIN SELECT RAISE(ABORT, 'template_revisions is append-only'); END;

CREATE TABLE proposals (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    state          TEXT    NOT NULL
        CHECK (state IN ('proposed', 'cooling', 'applied', 'rejected')),
    content        TEXT    NOT NULL,
    increases_risk INTEGER NOT NULL CHECK (increases_risk IN (0, 1)),
    approval_id    INTEGER REFERENCES approvals (id),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    applied_at     INTEGER,
    CHECK ((state = 'applied') = (applied_at IS NOT NULL)),
    CHECK ((state IN ('cooling', 'applied')) = (approval_id IS NOT NULL)),
    CHECK (state <> 'cooling' OR increases_risk = 1)
);
CREATE INDEX proposals_by_state ON proposals (state, id);
CREATE TABLE proposal_evidence (
    proposal_id INTEGER NOT NULL REFERENCES proposals (id),
    pos         INTEGER NOT NULL,
    trade_id    INTEGER NOT NULL REFERENCES trades (id),
    PRIMARY KEY (proposal_id, pos),
    UNIQUE (proposal_id, trade_id)
);
CREATE TRIGGER proposals_start_proposed BEFORE INSERT ON proposals
WHEN NEW.state <> 'proposed'
BEGIN SELECT RAISE(ABORT, 'proposals start proposed'); END;
CREATE TRIGGER proposals_no_replace BEFORE INSERT ON proposals
WHEN EXISTS (SELECT 1 FROM proposals WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'proposals rows are kept'); END;
CREATE TRIGGER proposals_no_delete BEFORE DELETE ON proposals
BEGIN SELECT RAISE(ABORT, 'proposals rows are kept'); END;
CREATE TRIGGER proposals_fixed BEFORE UPDATE ON proposals
WHEN NEW.id IS NOT OLD.id OR NEW.content IS NOT OLD.content
  OR NEW.increases_risk IS NOT OLD.increases_risk OR NEW.created_at IS NOT OLD.created_at
  OR (OLD.approval_id IS NOT NULL AND NEW.approval_id IS NOT OLD.approval_id)
  OR (OLD.applied_at IS NOT NULL AND NEW.applied_at IS NOT OLD.applied_at)
BEGIN SELECT RAISE(ABORT, 'proposal is fixed'); END;
CREATE TRIGGER proposals_transition BEFORE UPDATE ON proposals
WHEN NEW.state IS NOT OLD.state AND NOT (
     (OLD.state = 'proposed' AND NEW.state IN ('cooling', 'rejected'))
  OR (OLD.state = 'proposed' AND NEW.state = 'applied' AND OLD.increases_risk = 0)
  OR (OLD.state = 'cooling' AND NEW.state = 'applied'))
BEGIN SELECT RAISE(ABORT, 'proposal transition is not allowed'); END;
CREATE TRIGGER proposal_evidence_no_replace BEFORE INSERT ON proposal_evidence
WHEN EXISTS (SELECT 1 FROM proposal_evidence
             WHERE proposal_id = NEW.proposal_id
               AND (pos = NEW.pos OR trade_id = NEW.trade_id))
BEGIN SELECT RAISE(ABORT, 'proposal_evidence is append-only'); END;
CREATE TRIGGER proposal_evidence_no_update BEFORE UPDATE ON proposal_evidence
BEGIN SELECT RAISE(ABORT, 'proposal_evidence is append-only'); END;
CREATE TRIGGER proposal_evidence_no_delete BEFORE DELETE ON proposal_evidence
BEGIN SELECT RAISE(ABORT, 'proposal_evidence is append-only'); END;

CREATE TABLE paper_snapshots (
    seq   INTEGER PRIMARY KEY,
    at    INTEGER NOT NULL,
    state TEXT    NOT NULL
);
CREATE TRIGGER paper_snapshots_no_replace BEFORE INSERT ON paper_snapshots
WHEN EXISTS (SELECT 1 FROM paper_snapshots WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'paper_snapshots is append-only'); END;
CREATE TRIGGER paper_snapshots_no_update BEFORE UPDATE ON paper_snapshots
BEGIN SELECT RAISE(ABORT, 'paper_snapshots is append-only'); END;
CREATE TRIGGER paper_snapshots_no_delete BEFORE DELETE ON paper_snapshots
BEGIN SELECT RAISE(ABORT, 'paper_snapshots is append-only'); END;

CREATE TABLE key_hashes (
    generation INTEGER PRIMARY KEY CHECK (generation >= 1),
    sha256     BLOB    NOT NULL CHECK (typeof(sha256) = 'blob' AND length(sha256) = 32),
    salt       BLOB    NOT NULL CHECK (typeof(salt) = 'blob' AND length(salt) >= 16),
    set_at     INTEGER NOT NULL
);
CREATE TRIGGER key_hashes_next_generation BEFORE INSERT ON key_hashes
WHEN NEW.generation IS NOT (SELECT COALESCE(MAX(generation), 0) + 1 FROM key_hashes)
BEGIN SELECT RAISE(ABORT, 'key_hashes is append-only'); END;
CREATE TRIGGER key_hashes_no_update BEFORE UPDATE ON key_hashes
BEGIN SELECT RAISE(ABORT, 'key_hashes is append-only'); END;
CREATE TRIGGER key_hashes_no_delete BEFORE DELETE ON key_hashes
BEGIN SELECT RAISE(ABORT, 'key_hashes is append-only'); END;
";

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    /// 버전 1 ~ 5를 차례로 올린 메모리 DB.
    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        for step in super::super::MIGRATIONS {
            conn.execute_batch(step).unwrap();
        }
        conn
    }

    fn ok(conn: &Connection, sql: &str) {
        conn.execute_batch(sql).unwrap();
    }

    fn denied(conn: &Connection, sql: &str) {
        assert!(conn.execute_batch(sql).is_err(), "{sql}");
    }

    #[test]
    fn 개선안은_표에서도_상태표_밖으로_가지_않고_근거는_쌓기만_한다() {
        let conn = conn();
        ok(
            &conn,
            "INSERT INTO trades (symbol, account, state, created_at, updated_at)
                   VALUES ('005930', 'paper', 'discussing', 0, 0)",
        );
        ok(
            &conn,
            "INSERT INTO proposals (state, content, increases_risk, created_at, updated_at)
                   VALUES ('proposed', '{}', 1, 0, 0)",
        );
        ok(&conn, "INSERT INTO proposal_evidence VALUES (1, 0, 1)");
        denied(
            &conn,
            "INSERT OR REPLACE INTO proposal_evidence VALUES (1, 0, 1)",
        );
        denied(&conn, "DELETE FROM proposal_evidence");
        // 위험을 키우는 개선안은 승인 없이도, 승인이 있어도 바로 반영됨이 되지 않는다.
        denied(
            &conn,
            "UPDATE proposals SET state = 'applied', applied_at = 1",
        );
        denied(&conn, "UPDATE proposals SET content = '[]'");
        denied(&conn, "DELETE FROM proposals");
        ok(&conn, "UPDATE proposals SET state = 'rejected'");
        denied(&conn, "UPDATE proposals SET state = 'proposed'");
    }

    #[test]
    fn 템플릿_값은_모의중에서만_다음_판으로_간다() {
        let conn = conn();
        ok(
            &conn,
            "INSERT INTO templates (state, revision, created_at, updated_at)
                   VALUES ('simulating', 1, 0, 0)",
        );
        ok(
            &conn,
            "INSERT INTO template_revisions VALUES (1, 1, '{}', 0)",
        );
        denied(
            &conn,
            "INSERT INTO template_revisions VALUES (1, 3, '{}', 0)",
        );
        denied(&conn, "UPDATE templates SET revision = 2");
        denied(&conn, "UPDATE template_revisions SET value = '[]'");
        ok(
            &conn,
            "UPDATE templates SET state = 'promotion_pending', pass_revision = 1,
                   pass_record = '{}', pass_at = 1",
        );
        denied(
            &conn,
            "INSERT INTO template_revisions VALUES (1, 2, '{}', 0)",
        );
        denied(&conn, "DELETE FROM templates");
    }

    #[test]
    fn 접속_키_해시는_다음_세대로만_쌓인다() {
        let conn = conn();
        let hash = "x'00000000000000000000000000000000000000000000000000000000000000ff'";
        let salt = "x'0102030405060708090a0b0c0d0e0f10'";
        ok(
            &conn,
            &format!("INSERT INTO key_hashes VALUES (1, {hash}, {salt}, 0)"),
        );
        denied(
            &conn,
            &format!("INSERT INTO key_hashes VALUES (3, {hash}, {salt}, 0)"),
        );
        denied(
            &conn,
            &format!("INSERT INTO key_hashes VALUES (2, {hash}, x'01', 0)"),
        );
        denied(&conn, "UPDATE key_hashes SET set_at = 1");
        denied(&conn, "DELETE FROM key_hashes");
    }
}
