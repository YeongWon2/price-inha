//! 스키마 버전 0 → 1. 이미 나간 SQL이다. 고치지 않는다.

/// 기록 표(team_history · events · access_log)는 고치기 · 지우기를 SQLite가 막는다.
pub(super) const V1: &str = "
CREATE TABLE team_history (
    seq            INTEGER PRIMARY KEY,
    at             INTEGER NOT NULL,
    from_state     TEXT    NOT NULL,
    to_state       TEXT    NOT NULL,
    cause          TEXT    NOT NULL,
    detail         TEXT    NOT NULL,
    owner_approved INTEGER NOT NULL
);
CREATE TABLE trades (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol               TEXT    NOT NULL,
    account              TEXT    NOT NULL,
    state                TEXT    NOT NULL,
    created_at           INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL,
    hypothesis           TEXT,
    trigger_note         TEXT,
    entry                INTEGER,
    stop                 INTEGER,
    target               INTEGER,
    expected_value       INTEGER,
    decided_at           INTEGER,
    consensus_score      INTEGER,
    approval_number      INTEGER,
    approval_fingerprint TEXT
);
CREATE INDEX trades_by_state ON trades (state, id);
CREATE TABLE trade_sources (
    trade_id INTEGER NOT NULL REFERENCES trades (id),
    pos      INTEGER NOT NULL,
    name     TEXT    NOT NULL,
    known_at INTEGER NOT NULL,
    PRIMARY KEY (trade_id, pos)
);
CREATE TABLE trade_scores (
    trade_id INTEGER NOT NULL REFERENCES trades (id),
    pos      INTEGER NOT NULL,
    role     TEXT    NOT NULL,
    score    INTEGER NOT NULL,
    reason   TEXT    NOT NULL,
    PRIMARY KEY (trade_id, pos)
);
CREATE TABLE events (
    seq     INTEGER PRIMARY KEY,
    kind    TEXT NOT NULL,
    payload TEXT NOT NULL
);
CREATE TABLE access_log (
    seq    INTEGER PRIMARY KEY,
    at     INTEGER NOT NULL,
    origin TEXT    NOT NULL,
    action TEXT    NOT NULL
);
CREATE TRIGGER team_history_no_update BEFORE UPDATE ON team_history
BEGIN SELECT RAISE(ABORT, 'team_history is append-only'); END;
CREATE TRIGGER team_history_no_delete BEFORE DELETE ON team_history
BEGIN SELECT RAISE(ABORT, 'team_history is append-only'); END;
CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER events_no_delete BEFORE DELETE ON events
BEGIN SELECT RAISE(ABORT, 'events is append-only'); END;
CREATE TRIGGER access_log_no_update BEFORE UPDATE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
CREATE TRIGGER access_log_no_delete BEFORE DELETE ON access_log
BEGIN SELECT RAISE(ABORT, 'access_log is append-only'); END;
";
