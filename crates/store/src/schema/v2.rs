//! 스키마 버전 1 → 2(2차 작업). 이미 나간 SQL이다. 고치지 않는다.

/// 버전 2(2차 작업). 버전 1의 표와 줄은 그대로 두고 더하기만 한다.
///
/// - 매매에 막힌 이유 칸 둘. 버전 1에서 막힌 매매는 이유가 비어 있다.
/// - 한 번이라도 매매에 붙은 승인 번호의 목록(`approval_numbers`). 번호의 주인은 처음 붙인 매매이고
///   바뀌거나 지워지지 않는다. 버전 1 파일에 같은 번호가 여러 매매에 붙어 있으면 번호가 가장 작은
///   매매가 주인이 된다(매매 줄은 고치지 않는다).
/// - 기록 표에 "이미 있는 번호로 넣기"를 막는 트리거. `INSERT OR REPLACE`는 지우기 트리거를
///   부르지 않고 줄을 갈아 끼우므로, 넣기 전에 같은 번호가 있는지 보고 막는다.
///   기록 표의 줄은 건드리지 않으므로 버전 1의 보호 트리거와 부딪히지 않는다.
pub(super) const V2: &str = "
ALTER TABLE trades ADD COLUMN block_kind TEXT;
ALTER TABLE trades ADD COLUMN block_detail TEXT;
CREATE TABLE approval_numbers (
    number   INTEGER PRIMARY KEY,
    trade_id INTEGER NOT NULL REFERENCES trades (id)
);
INSERT OR IGNORE INTO approval_numbers (number, trade_id)
SELECT approval_number, id FROM trades WHERE approval_number IS NOT NULL ORDER BY id;
CREATE TRIGGER approval_numbers_no_replace BEFORE INSERT ON approval_numbers
WHEN EXISTS (SELECT 1 FROM approval_numbers WHERE number = NEW.number)
BEGIN SELECT RAISE(ABORT, 'approval number is already used'); END;
CREATE TRIGGER approval_numbers_no_update BEFORE UPDATE ON approval_numbers
BEGIN SELECT RAISE(ABORT, 'approval_numbers is append-only'); END;
CREATE TRIGGER approval_numbers_no_delete BEFORE DELETE ON approval_numbers
BEGIN SELECT RAISE(ABORT, 'approval_numbers is append-only'); END;
CREATE TRIGGER team_history_no_replace BEFORE INSERT ON team_history
WHEN EXISTS (SELECT 1 FROM team_history WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'team_history is append-only'); END;
CREATE TRIGGER events_no_replace BEFORE INSERT ON events
WHEN EXISTS (SELECT 1 FROM events WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER access_log_no_replace BEFORE INSERT ON access_log
WHEN EXISTS (SELECT 1 FROM access_log WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
";
