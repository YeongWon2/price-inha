//! 버전 7 → 8 (후보 일괄 처리): 매매 계획(방향 · 수량) · 매매 상태 바뀜 이력 · 지식 조각 대체 막기 보강 ·
//! 개선안 위험 표시 올리기.
//!
//! 표가 스스로 지키는 것(어떤 SQL로도):
//! - `trade_plans`: 매매 하나에 한 줄. 방향은 한 번 정하면 바뀌지 않고, 수량은 승인이 붙었거나 승인대기를
//!   지난 매매에서는 넣거나 바꿀 수 없다. 지우기 · 대체 거부.
//! - `trade_state_changes`: 매매를 만들거나 상태를 바꾸면 트리거가 한 줄씩 남긴다. 쌓기만 한다.
//!   이 버전 전에 있던 매매의 지난 바뀜은 없다(알 수 없다).
//! - `knowledge_chunks_no_replace`: 번호뿐 아니라 `(kind, record_ref, ordinal)`이 겹치는 대체도 거부한다
//!   (대체는 옛 줄을 지우기 트리거 없이 지워 색인 · 임베딩이 고아로 남는다).
//! - `proposals_fixed`: 위험 표시는 제안됨인 동안 거짓 → 참으로만 바뀐다(낮추는 쪽은 냉각을 건너뛰게 하므로 막는다).

/// 버전 7에서 8로 올리는 SQL.
pub(super) const V8: &str = "
CREATE TABLE trade_plans (
    trade_id INTEGER PRIMARY KEY REFERENCES trades (id),
    side     TEXT    CHECK (side IS NULL OR side IN ('buy', 'sell')),
    quantity INTEGER CHECK (quantity IS NULL OR quantity > 0)
);
CREATE TRIGGER trade_plans_no_replace BEFORE INSERT ON trade_plans
WHEN EXISTS (SELECT 1 FROM trade_plans WHERE trade_id = NEW.trade_id)
BEGIN SELECT RAISE(ABORT, 'trade_plans rows are kept'); END;
CREATE TRIGGER trade_plans_no_delete BEFORE DELETE ON trade_plans
BEGIN SELECT RAISE(ABORT, 'trade_plans rows are kept'); END;
CREATE TRIGGER trade_plans_side_fixed BEFORE UPDATE ON trade_plans
WHEN NEW.trade_id IS NOT OLD.trade_id
  OR (OLD.side IS NOT NULL AND NEW.side IS NOT OLD.side)
BEGIN SELECT RAISE(ABORT, 'trade side is fixed'); END;
CREATE TRIGGER trade_plans_quantity_locked_insert BEFORE INSERT ON trade_plans
WHEN NEW.quantity IS NOT NULL AND EXISTS (
  SELECT 1 FROM trades WHERE id = NEW.trade_id
    AND (approval_number IS NOT NULL
         OR state NOT IN ('discussing', 'judged', 'awaiting_approval')))
BEGIN SELECT RAISE(ABORT, 'trade quantity is locked'); END;
CREATE TRIGGER trade_plans_quantity_locked_update BEFORE UPDATE ON trade_plans
WHEN NEW.quantity IS NOT OLD.quantity AND EXISTS (
  SELECT 1 FROM trades WHERE id = OLD.trade_id
    AND (approval_number IS NOT NULL
         OR state NOT IN ('discussing', 'judged', 'awaiting_approval')))
BEGIN SELECT RAISE(ABORT, 'trade quantity is locked'); END;

CREATE TABLE trade_state_changes (
    seq        INTEGER PRIMARY KEY AUTOINCREMENT,
    trade_id   INTEGER NOT NULL REFERENCES trades (id),
    from_state TEXT,
    to_state   TEXT    NOT NULL,
    at         INTEGER NOT NULL
);
CREATE INDEX trade_state_changes_by_trade ON trade_state_changes (trade_id, seq);
CREATE INDEX trade_state_changes_by_at ON trade_state_changes (at, seq);
CREATE TRIGGER trade_state_changes_no_replace BEFORE INSERT ON trade_state_changes
WHEN EXISTS (SELECT 1 FROM trade_state_changes WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'trade_state_changes is append-only'); END;
CREATE TRIGGER trade_state_changes_no_update BEFORE UPDATE ON trade_state_changes
BEGIN SELECT RAISE(ABORT, 'trade_state_changes is append-only'); END;
CREATE TRIGGER trade_state_changes_no_delete BEFORE DELETE ON trade_state_changes
BEGIN SELECT RAISE(ABORT, 'trade_state_changes is append-only'); END;
CREATE TRIGGER trades_record_created AFTER INSERT ON trades
BEGIN
    INSERT INTO trade_state_changes (trade_id, from_state, to_state, at)
    VALUES (NEW.id, NULL, NEW.state, NEW.created_at);
END;
CREATE TRIGGER trades_record_state AFTER UPDATE OF state ON trades
WHEN NEW.state IS NOT OLD.state
BEGIN
    INSERT INTO trade_state_changes (trade_id, from_state, to_state, at)
    VALUES (NEW.id, OLD.state, NEW.state, NEW.updated_at);
END;

DROP TRIGGER knowledge_chunks_no_replace;
CREATE TRIGGER knowledge_chunks_no_replace BEFORE INSERT ON knowledge_chunks
WHEN EXISTS (SELECT 1 FROM knowledge_chunks
             WHERE id = NEW.id
                OR (kind = NEW.kind AND record_ref = NEW.record_ref AND ordinal = NEW.ordinal))
BEGIN SELECT RAISE(ABORT, 'knowledge_chunks rows are replaced by re-indexing'); END;

DROP TRIGGER proposals_fixed;
CREATE TRIGGER proposals_fixed BEFORE UPDATE ON proposals
WHEN NEW.id IS NOT OLD.id OR NEW.content IS NOT OLD.content
  OR (NEW.increases_risk IS NOT OLD.increases_risk AND NOT (
        OLD.state = 'proposed' AND NEW.state = 'proposed'
        AND OLD.increases_risk = 0 AND NEW.increases_risk = 1))
  OR NEW.created_at IS NOT OLD.created_at
  OR (OLD.approval_id IS NOT NULL AND NEW.approval_id IS NOT OLD.approval_id)
  OR (OLD.applied_at IS NOT NULL AND NEW.applied_at IS NOT OLD.applied_at)
BEGIN SELECT RAISE(ABORT, 'proposal is fixed'); END;
";
