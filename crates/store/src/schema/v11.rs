//! 버전 10에서 11로 올린다(19차: 내 캐릭터 · 담당 의견과 팀장 요약 · 공지 · 결정 메모).
//!
//! 새 표와 트리거만 더한다(옛 표는 그대로).
//! - `player_profile`: 내 캐릭터. 설정이라 덮어쓴다. 줄은 `id = 1` 하나뿐이다(지우기는 막지 않는다).
//! - `role_opinions` · `trade_summaries`: 판단 때 남기는 기록. 지우기 · 고치기 · 다시 넣기를 막는다.
//! - `notices`: 공지 글. 지우기 · 고치기 · 다시 넣기를 막는다.
//! - `notice_acks`: 담당별 확인. 기다림에서 한 번만 바뀌고 지우지 못한다.
//! - `decision_memos`: 결정 메모. 만든 칸은 고치지 못하고, 정산 대기에서 한 번만 바뀌며 지우지 못한다.
//!
//! 바꿔치기(`INSERT OR REPLACE` · `REPLACE INTO`)는 충돌한 줄을 지울 때 지우기 막기 트리거가 돌지 않으므로
//! (recursive_triggers 꺼짐) 키나 rowid가 겹치는 넣기를 표가 스스로 거부한다(판 6과 같은 모양).

use rusqlite::Connection;

use crate::error::StoreError;

/// 옮기기 끝에 파일이 온전한지 본다(`PRAGMA integrity_check`). 판 10 이하에서 올라올 때마다 본다
/// (판 10 단계가 표 정의 글자를 직접 고치므로). ok가 아니면 옮기기는 실패하고(묶음이 돌아간다)
/// 파일은 옛 판 그대로다.
pub(super) fn integrity_ok(tx: &Connection) -> Result<(), StoreError> {
    let result: String = tx.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(StoreError::Incompatible(format!(
            "판 11로 옮긴 뒤 무결성 검사가 실패했다: {result}"
        )));
    }
    Ok(())
}

/// 버전 10에서 11로 올리는 SQL.
pub(super) const V11: &str = "
CREATE TABLE player_profile (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    name       TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 50),
    look       TEXT    CHECK (look IS NULL OR length(look) > 0),
    spot_x     INTEGER CHECK (spot_x IS NULL OR spot_x >= 0),
    spot_y     INTEGER CHECK (spot_y IS NULL OR spot_y >= 0),
    facing     TEXT    CHECK (facing IS NULL OR facing IN ('up', 'down', 'left', 'right')),
    updated_at INTEGER NOT NULL,
    CHECK ((spot_x IS NULL) = (spot_y IS NULL) AND (spot_x IS NULL) = (facing IS NULL))
);

CREATE TABLE trade_summaries (
    trade_id INTEGER PRIMARY KEY REFERENCES trades (id),
    summary  TEXT    NOT NULL,
    reversal TEXT    NOT NULL
);
CREATE TRIGGER trade_summaries_with_decision BEFORE INSERT ON trade_summaries
WHEN (SELECT decided_at FROM trades WHERE id = NEW.trade_id) IS NULL
BEGIN SELECT RAISE(ABORT, 'trade summary is kept with the decision'); END;
CREATE TRIGGER trade_summaries_no_replace BEFORE INSERT ON trade_summaries
WHEN EXISTS (SELECT 1 FROM trade_summaries WHERE trade_id = NEW.trade_id)
BEGIN SELECT RAISE(ABORT, 'trade_summaries is append-only'); END;
CREATE TRIGGER trade_summaries_no_update BEFORE UPDATE ON trade_summaries
BEGIN SELECT RAISE(ABORT, 'trade_summaries is append-only'); END;
CREATE TRIGGER trade_summaries_no_delete BEFORE DELETE ON trade_summaries
BEGIN SELECT RAISE(ABORT, 'trade_summaries rows are kept'); END;

CREATE TABLE role_opinions (
    id           INTEGER PRIMARY KEY,
    trade_id     INTEGER NOT NULL REFERENCES trades (id),
    role         TEXT    NOT NULL CHECK (length(role) > 0),
    stance       TEXT    CHECK (stance IS NULL OR stance IN ('bullish', 'bearish', 'neutral')),
    conviction   TEXT    CHECK (conviction IS NULL OR conviction IN ('low', 'medium', 'high')),
    reason       TEXT    NOT NULL,
    horizon_days INTEGER CHECK (horizon_days IS NULL OR horizon_days >= 1),
    abstained    INTEGER NOT NULL CHECK (abstained IN (0, 1)),
    CHECK (CASE WHEN abstained = 1 THEN stance IS NULL AND conviction IS NULL
                ELSE stance IS NOT NULL AND conviction IS NOT NULL END),
    UNIQUE (trade_id, role)
);
CREATE TRIGGER role_opinions_with_decision BEFORE INSERT ON role_opinions
WHEN (SELECT decided_at FROM trades WHERE id = NEW.trade_id) IS NULL
  OR EXISTS (SELECT 1 FROM trade_summaries WHERE trade_id = NEW.trade_id)
BEGIN SELECT RAISE(ABORT, 'role opinions are kept with the decision'); END;
CREATE TRIGGER role_opinions_no_replace BEFORE INSERT ON role_opinions
WHEN EXISTS (SELECT 1 FROM role_opinions
             WHERE id = NEW.id OR (trade_id = NEW.trade_id AND role = NEW.role))
BEGIN SELECT RAISE(ABORT, 'role_opinions is append-only'); END;
CREATE TRIGGER role_opinions_no_update BEFORE UPDATE ON role_opinions
BEGIN SELECT RAISE(ABORT, 'role_opinions is append-only'); END;
CREATE TRIGGER role_opinions_no_delete BEFORE DELETE ON role_opinions
BEGIN SELECT RAISE(ABORT, 'role_opinions rows are kept'); END;

CREATE TABLE notices (
    id      INTEGER PRIMARY KEY,
    text    TEXT    NOT NULL CHECK (length(text) BETWEEN 1 AND 500),
    sent_at INTEGER NOT NULL
);
CREATE INDEX notices_by_time ON notices (sent_at, id);
CREATE TRIGGER notices_no_replace BEFORE INSERT ON notices
WHEN EXISTS (SELECT 1 FROM notices WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'notices is append-only'); END;
CREATE TRIGGER notices_no_update BEFORE UPDATE ON notices
BEGIN SELECT RAISE(ABORT, 'notices is append-only'); END;
CREATE TRIGGER notices_no_delete BEFORE DELETE ON notices
BEGIN SELECT RAISE(ABORT, 'notices rows are kept'); END;

CREATE TABLE notice_acks (
    id          INTEGER PRIMARY KEY,
    notice_id   INTEGER NOT NULL REFERENCES notices (id),
    role        TEXT    NOT NULL CHECK (length(role) > 0),
    status      TEXT    NOT NULL CHECK (status IN ('waiting', 'answered', 'capped', 'failed')),
    answer      TEXT,
    answered_at INTEGER,
    CHECK ((status = 'waiting') = (answer IS NULL)
       AND (status = 'waiting') = (answered_at IS NULL)),
    UNIQUE (notice_id, role)
);
CREATE TRIGGER notice_acks_start_waiting BEFORE INSERT ON notice_acks
WHEN NEW.status IS NOT 'waiting'
BEGIN SELECT RAISE(ABORT, 'notice acks start waiting'); END;
CREATE TRIGGER notice_acks_no_replace BEFORE INSERT ON notice_acks
WHEN EXISTS (SELECT 1 FROM notice_acks
             WHERE id = NEW.id OR (notice_id = NEW.notice_id AND role = NEW.role))
BEGIN SELECT RAISE(ABORT, 'notice_acks rows are kept'); END;
CREATE TRIGGER notice_acks_change_once BEFORE UPDATE ON notice_acks
WHEN OLD.status IS NOT 'waiting' OR NEW.status IS 'waiting'
  OR NEW.id IS NOT OLD.id OR NEW.notice_id IS NOT OLD.notice_id OR NEW.role IS NOT OLD.role
BEGIN SELECT RAISE(ABORT, 'notice ack changes once from waiting'); END;
CREATE TRIGGER notice_acks_no_delete BEFORE DELETE ON notice_acks
BEGIN SELECT RAISE(ABORT, 'notice_acks rows are kept'); END;

CREATE TABLE decision_memos (
    id           INTEGER PRIMARY KEY,
    trade_id     INTEGER NOT NULL UNIQUE REFERENCES trades (id),
    date         TEXT    NOT NULL CHECK (length(date) = 10),
    symbol       TEXT    NOT NULL,
    side         TEXT    NOT NULL CHECK (side IN ('buy', 'sell')),
    state        TEXT    NOT NULL CHECK (state IN ('pending', 'settled', 'unsettleable')),
    return_bp    INTEGER,
    excess_bp    INTEGER,
    holding_days INTEGER CHECK (holding_days IS NULL OR holding_days >= 0),
    lesson       TEXT,
    created_at   INTEGER NOT NULL,
    settled_at   INTEGER,
    CHECK (CASE state
        WHEN 'settled' THEN return_bp IS NOT NULL AND holding_days IS NOT NULL
                        AND length(lesson) > 0 AND settled_at IS NOT NULL
        ELSE return_bp IS NULL AND excess_bp IS NULL AND holding_days IS NULL AND lesson IS NULL
             AND (settled_at IS NULL) = (state = 'pending') END)
);
CREATE INDEX decision_memos_by_date ON decision_memos (state, date, id);
CREATE TRIGGER decision_memos_judged_only BEFORE INSERT ON decision_memos
WHEN NEW.state IS NOT 'pending'
  OR (SELECT decided_at FROM trades WHERE id = NEW.trade_id) IS NULL
  OR NEW.symbol IS NOT (SELECT symbol FROM trades WHERE id = NEW.trade_id)
BEGIN SELECT RAISE(ABORT, 'decision memo starts pending for a judged trade'); END;
CREATE TRIGGER decision_memos_no_replace BEFORE INSERT ON decision_memos
WHEN EXISTS (SELECT 1 FROM decision_memos WHERE id = NEW.id OR trade_id = NEW.trade_id)
BEGIN SELECT RAISE(ABORT, 'decision_memos rows are kept'); END;
CREATE TRIGGER decision_memos_settle_once BEFORE UPDATE ON decision_memos
WHEN OLD.state IS NOT 'pending' OR NEW.state IS 'pending'
  OR NEW.id IS NOT OLD.id OR NEW.trade_id IS NOT OLD.trade_id OR NEW.date IS NOT OLD.date
  OR NEW.symbol IS NOT OLD.symbol OR NEW.side IS NOT OLD.side
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'decision memo is settled once'); END;
CREATE TRIGGER decision_memos_no_delete BEFORE DELETE ON decision_memos
BEGIN SELECT RAISE(ABORT, 'decision_memos rows are kept'); END;
";

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = super::super::open(&dir.path().join("pi.sqlite")).unwrap();
        (dir, conn)
    }

    #[test]
    fn 내_캐릭터_표는_두_줄이_되지_못한다() {
        let (_dir, conn) = fresh();
        conn.execute(
            "INSERT INTO player_profile (id, name, updated_at) VALUES (1, '나', 1)",
            [],
        )
        .unwrap();
        assert!(
            conn.execute(
                "INSERT INTO player_profile (id, name, updated_at) VALUES (2, '나', 1)",
                [],
            )
            .is_err()
        );
    }

    #[test]
    fn 공지_확인은_기다림에서_한_번만_바뀌고_담당을_바꾸지_못한다() {
        let (_dir, conn) = fresh();
        conn.execute_batch(
            "INSERT INTO notices (id, text, sent_at) VALUES (1, '공지', 1);
             INSERT INTO notice_acks (notice_id, role, status) VALUES (1, 'a', 'waiting');",
        )
        .unwrap();
        let bad_start = "INSERT INTO notice_acks (notice_id, role, status, answer, answered_at) \
                         VALUES (1, 'b', 'answered', 'x', 2)";
        assert!(conn.execute(bad_start, []).is_err());
        assert!(
            conn.execute("UPDATE notice_acks SET role = 'z' WHERE role = 'a'", [])
                .is_err()
        );
        let answer = "UPDATE notice_acks SET status = 'failed', answer = '', answered_at = 3";
        conn.execute(answer, []).unwrap();
        assert!(
            conn.execute(
                "UPDATE notice_acks SET status = 'answered', answer = 'y', answered_at = 4",
                [],
            )
            .is_err()
        );
    }

    #[test]
    fn 결정_메모는_판단된_매매에만_생기고_만든_칸을_고치지_못한다() {
        let (_dir, conn) = fresh();
        conn.execute_batch(
            "INSERT INTO trades (id, symbol, account, state, created_at, updated_at)
             VALUES (1, '005930', 'paper', 'discussing', 0, 0);",
        )
        .unwrap();
        let insert = "INSERT INTO decision_memos (trade_id, date, symbol, side, state, created_at) \
                      VALUES (1, '2026-11-24', '005930', 'buy', 'pending', 1)";
        assert!(conn.execute(insert, []).is_err(), "판단 전 매매");
        conn.execute("UPDATE trades SET decided_at = 5 WHERE id = 1", [])
            .unwrap();
        conn.execute(insert, []).unwrap();
        assert!(
            conn.execute("UPDATE decision_memos SET date = '2026-11-25'", [])
                .is_err()
        );
        conn.execute(
            "UPDATE decision_memos SET state = 'unsettleable', settled_at = 9",
            [],
        )
        .unwrap();
        assert!(
            conn.execute(
                "UPDATE decision_memos SET state = 'pending', settled_at = NULL",
                []
            )
            .is_err()
        );
    }
}
