//! 버전 8 → 9 (13차): 지식 조각 줄 바꿔치기 막기 · 매매 방향은 열린 상태에서만 정하기.
//!
//! 표가 스스로 지키는 것(어떤 SQL로도):
//! - `knowledge_chunks_no_overlap_update`: 갱신으로 `(kind, record_ref, ordinal)`이나 번호가 다른 줄과 겹치게 되면
//!   거부한다(`UPDATE OR REPLACE`는 겹친 줄을 지우기 트리거 없이 지워 색인 · 임베딩이 고아로 남는다).
//! - `trade_plans_side_set_open`: 방향을 NULL에서 값으로 정하는 것은 매매가 논의중 · 판단됨일 때만 된다.

/// 버전 8에서 9로 올리는 SQL.
pub(super) const V9: &str = "
CREATE TRIGGER knowledge_chunks_no_overlap_update BEFORE UPDATE ON knowledge_chunks
WHEN EXISTS (SELECT 1 FROM knowledge_chunks
             WHERE id IS NOT OLD.id
               AND (id = NEW.id
                    OR (kind = NEW.kind AND record_ref = NEW.record_ref AND ordinal = NEW.ordinal)))
BEGIN SELECT RAISE(ABORT, 'knowledge_chunks rows are replaced by re-indexing'); END;

CREATE TRIGGER trade_plans_side_set_open BEFORE UPDATE ON trade_plans
WHEN OLD.side IS NULL AND NEW.side IS NOT NULL AND EXISTS (
  SELECT 1 FROM trades WHERE id = OLD.trade_id
    AND state NOT IN ('discussing', 'judged'))
BEGIN SELECT RAISE(ABORT, 'trade side can only be set while open'); END;
CREATE TRIGGER trade_plans_side_set_open_insert BEFORE INSERT ON trade_plans
WHEN NEW.side IS NOT NULL AND EXISTS (
  SELECT 1 FROM trades WHERE id = NEW.trade_id
    AND state NOT IN ('discussing', 'judged'))
BEGIN SELECT RAISE(ABORT, 'trade side can only be set while open'); END;
";
