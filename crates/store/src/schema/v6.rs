//! 버전 5 → 6 (10차): 에이전트 세션 · 메시지 · 팀 논의 방 로그 · 트리거 작업 상태와 영수증 · 자료.
//!
//! 표가 스스로 지키는 것(어떤 SQL로도):
//! - 메시지는 쌓기만 한다(고치기 · 지우기 막음). 도구 결과는 호출 id가 있고, 도구 호출은 assistant만 가진다.
//!   같은 담당 · 같은 제목의 열린 세션은 하나뿐이다.
//! - 방 로그는 쌓기만 한다. 방마다 번호가 1부터 빠짐없이 커지고 `(방, event_id)`가 겹치지 않는다.
//!   쓴 쪽의 종류와 이벤트 종류의 짝은 `room_event_kinds` 표에 있는 것만 들어간다(표 자체도 고정).
//! - 트리거 영수증은 `(job, slot)`이 겹치지 않고, 상태는 넷 가운데 하나이며, `running`이면 닫은 시각이
//!   비어 있고 아니면 채워져 있다. 작업마다 `running`은 하나뿐이고, 닫힌 영수증은 다시 `running`이
//!   되지 않으며, 키와 연 시각은 바뀌지 않는다.
//! - 자료(`facts`)는 고치거나 지울 수 없다. `trade_facts`에는 판단 시각이 있는 매매와 그 시각까지
//!   알게 된 자료만 들어가고, 붙인 줄은 고치거나 지울 수 없다. 자료가 붙은 매매의 판단 시각을
//!   그 자료보다 앞으로 옮길 수도 없다.

/// 버전 5에서 6으로 올리는 SQL.
pub(super) const V6: &str = "
CREATE TABLE agent_sessions (
    id              INTEGER PRIMARY KEY,
    agent           TEXT    NOT NULL,
    title           TEXT    NOT NULL,
    started_at      INTEGER NOT NULL,
    ended_at        INTEGER,
    end_reason      TEXT,
    message_count   INTEGER NOT NULL DEFAULT 0 CHECK (message_count >= 0),
    tool_call_count INTEGER NOT NULL DEFAULT 0 CHECK (tool_call_count >= 0),
    CHECK ((ended_at IS NULL) = (end_reason IS NULL))
);
CREATE UNIQUE INDEX agent_sessions_open ON agent_sessions (agent, title) WHERE ended_at IS NULL;
CREATE INDEX agent_sessions_by_agent ON agent_sessions (agent, id);
CREATE TRIGGER agent_sessions_no_delete BEFORE DELETE ON agent_sessions
BEGIN SELECT RAISE(ABORT, 'agent_sessions rows are kept'); END;
CREATE TRIGGER agent_sessions_fixed BEFORE UPDATE ON agent_sessions
WHEN NEW.id IS NOT OLD.id OR NEW.agent IS NOT OLD.agent OR NEW.title IS NOT OLD.title
  OR NEW.started_at IS NOT OLD.started_at
  OR (OLD.ended_at IS NOT NULL
      AND (NEW.ended_at IS NOT OLD.ended_at OR NEW.end_reason IS NOT OLD.end_reason))
BEGIN SELECT RAISE(ABORT, 'agent session is fixed'); END;

CREATE TABLE agent_messages (
    id            INTEGER PRIMARY KEY,
    session_id    INTEGER NOT NULL REFERENCES agent_sessions (id),
    role          TEXT    NOT NULL CHECK (role IN ('system', 'user', 'assistant', 'tool')),
    content       TEXT,
    api_content   TEXT,
    tool_calls    TEXT,
    tool_call_id  TEXT,
    tool_name     TEXT,
    finish_reason TEXT,
    reasoning     TEXT,
    at            INTEGER NOT NULL,
    CHECK (role <> 'tool' OR tool_call_id IS NOT NULL),
    CHECK (role = 'assistant' OR tool_calls IS NULL)
);
CREATE INDEX agent_messages_by_session ON agent_messages (session_id, id);
CREATE TRIGGER agent_messages_no_replace BEFORE INSERT ON agent_messages
WHEN EXISTS (SELECT 1 FROM agent_messages WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'agent_messages is append-only'); END;
CREATE TRIGGER agent_messages_no_update BEFORE UPDATE ON agent_messages
BEGIN SELECT RAISE(ABORT, 'agent_messages is append-only'); END;
CREATE TRIGGER agent_messages_no_delete BEFORE DELETE ON agent_messages
BEGIN SELECT RAISE(ABORT, 'agent_messages is append-only'); END;

CREATE TABLE room_event_kinds (
    actor_kind TEXT NOT NULL,
    kind       TEXT NOT NULL,
    PRIMARY KEY (actor_kind, kind)
);
INSERT INTO room_event_kinds (actor_kind, kind) VALUES
    ('user', 'message.user'),
    ('member', 'message.member'),
    ('gateway', 'member.unavailable'),
    ('gateway', 'room.activity'),
    ('gateway', 'room.assignment'),
    ('gateway', 'room.continued'),
    ('gateway', 'room.routed'),
    ('gateway', 'room.stop_requested'),
    ('gateway', 'turn.started'),
    ('gateway', 'turn.settled'),
    ('gateway', 'turn.failed'),
    ('gateway', 'turn.cancelled'),
    ('gateway', 'turn.deferred'),
    ('gateway', 'turn.reassigned'),
    ('system', 'authority.claimed'),
    ('system', 'authority.lost'),
    ('system', 'room.context_set'),
    ('system', 'room.created'),
    ('system', 'room.disbanded'),
    ('system', 'room.members_changed'),
    ('system', 'room.renamed');
CREATE TRIGGER room_event_kinds_no_insert BEFORE INSERT ON room_event_kinds
BEGIN SELECT RAISE(ABORT, 'room_event_kinds is fixed'); END;
CREATE TRIGGER room_event_kinds_no_update BEFORE UPDATE ON room_event_kinds
BEGIN SELECT RAISE(ABORT, 'room_event_kinds is fixed'); END;
CREATE TRIGGER room_event_kinds_no_delete BEFORE DELETE ON room_event_kinds
BEGIN SELECT RAISE(ABORT, 'room_event_kinds is fixed'); END;

CREATE TABLE room_events (
    room       TEXT    NOT NULL,
    seq        INTEGER NOT NULL CHECK (seq >= 1),
    event_id   TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    actor_kind TEXT    NOT NULL CHECK (actor_kind IN ('user', 'member', 'gateway', 'system')),
    actor_id   TEXT    NOT NULL,
    payload    TEXT    NOT NULL,
    at         INTEGER NOT NULL,
    PRIMARY KEY (room, seq),
    UNIQUE (room, event_id)
);
CREATE TRIGGER room_events_allowed BEFORE INSERT ON room_events
WHEN NOT EXISTS (SELECT 1 FROM room_event_kinds
                 WHERE actor_kind = NEW.actor_kind AND kind = NEW.kind)
BEGIN SELECT RAISE(ABORT, 'room event kind is not allowed for this actor'); END;
CREATE TRIGGER room_events_next_seq BEFORE INSERT ON room_events
WHEN NEW.seq IS NOT COALESCE((SELECT MAX(seq) FROM room_events WHERE room = NEW.room), 0) + 1
BEGIN SELECT RAISE(ABORT, 'room seq must be the next number'); END;
CREATE TRIGGER room_events_no_update BEFORE UPDATE ON room_events
BEGIN SELECT RAISE(ABORT, 'room_events is append-only'); END;
CREATE TRIGGER room_events_no_delete BEFORE DELETE ON room_events
BEGIN SELECT RAISE(ABORT, 'room_events is append-only'); END;

CREATE TABLE trigger_jobs (
    job   TEXT NOT NULL PRIMARY KEY,
    state TEXT NOT NULL
);

CREATE TABLE trigger_receipts (
    job         TEXT    NOT NULL,
    slot        INTEGER NOT NULL,
    status      TEXT    NOT NULL
        CHECK (status IN ('running', 'ok', 'error', 'interrupted')),
    delivery    TEXT    NOT NULL CHECK (delivery IN ('not_started', 'started')),
    started_at  INTEGER NOT NULL,
    finished_at INTEGER,
    UNIQUE (job, slot),
    CHECK ((status = 'running') = (finished_at IS NULL))
);
CREATE UNIQUE INDEX trigger_receipts_one_running ON trigger_receipts (job)
    WHERE status = 'running';
CREATE TRIGGER trigger_receipts_start_running BEFORE INSERT ON trigger_receipts
WHEN NEW.status <> 'running' OR NEW.delivery <> 'not_started'
BEGIN SELECT RAISE(ABORT, 'trigger receipts start running'); END;
CREATE TRIGGER trigger_receipts_fixed BEFORE UPDATE ON trigger_receipts
WHEN NEW.job IS NOT OLD.job OR NEW.slot IS NOT OLD.slot OR NEW.started_at IS NOT OLD.started_at
  OR (OLD.status <> 'running' AND NEW.status = 'running')
  OR (OLD.delivery = 'started' AND NEW.delivery <> 'started')
BEGIN SELECT RAISE(ABORT, 'trigger receipt is fixed'); END;
CREATE TRIGGER trigger_receipts_no_delete BEFORE DELETE ON trigger_receipts
BEGIN SELECT RAISE(ABORT, 'trigger_receipts rows are kept'); END;

CREATE TABLE facts (
    id           INTEGER PRIMARY KEY,
    kind         TEXT    NOT NULL,
    subject      TEXT    NOT NULL,
    label        TEXT    NOT NULL,
    period       TEXT,
    value_kind   TEXT    NOT NULL CHECK (value_kind IN ('number', 'text')),
    value        TEXT    NOT NULL,
    unit         TEXT,
    reference    TEXT,
    source_name  TEXT    NOT NULL,
    source_url   TEXT    NOT NULL,
    published_on TEXT,
    known_at     INTEGER NOT NULL
);
CREATE INDEX facts_by_subject ON facts (subject, known_at, id);
CREATE TRIGGER facts_no_replace BEFORE INSERT ON facts
WHEN EXISTS (SELECT 1 FROM facts WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'facts is append-only'); END;
CREATE TRIGGER facts_no_update BEFORE UPDATE ON facts
BEGIN SELECT RAISE(ABORT, 'facts is append-only'); END;
CREATE TRIGGER facts_no_delete BEFORE DELETE ON facts
BEGIN SELECT RAISE(ABORT, 'facts is append-only'); END;

CREATE TABLE trade_facts (
    trade_id INTEGER NOT NULL REFERENCES trades (id),
    fact_id  INTEGER NOT NULL REFERENCES facts (id),
    UNIQUE (trade_id, fact_id)
);
CREATE TRIGGER trade_facts_known_by_decision BEFORE INSERT ON trade_facts
WHEN NOT EXISTS (
    SELECT 1 FROM trades AS t JOIN facts AS f ON f.id = NEW.fact_id
    WHERE t.id = NEW.trade_id AND t.decided_at IS NOT NULL AND f.known_at <= t.decided_at)
BEGIN SELECT RAISE(ABORT, 'trade fact must be known by the decision time'); END;
CREATE TRIGGER trade_facts_no_replace BEFORE INSERT ON trade_facts
WHEN EXISTS (SELECT 1 FROM trade_facts WHERE trade_id = NEW.trade_id AND fact_id = NEW.fact_id)
BEGIN SELECT RAISE(ABORT, 'trade_facts is append-only'); END;
CREATE TRIGGER trade_facts_no_update BEFORE UPDATE ON trade_facts
BEGIN SELECT RAISE(ABORT, 'trade_facts is append-only'); END;
CREATE TRIGGER trade_facts_no_delete BEFORE DELETE ON trade_facts
BEGIN SELECT RAISE(ABORT, 'trade_facts is append-only'); END;
CREATE TRIGGER trades_decision_keeps_facts BEFORE UPDATE OF decided_at ON trades
WHEN EXISTS (
    SELECT 1 FROM trade_facts AS tf JOIN facts AS f ON f.id = tf.fact_id
    WHERE tf.trade_id = NEW.id AND (NEW.decided_at IS NULL OR f.known_at > NEW.decided_at))
BEGIN SELECT RAISE(ABORT, 'trade decision time must not precede attached facts'); END;

-- 바꿔치기(`INSERT OR REPLACE` · `REPLACE INTO`) 막기: 충돌한 줄을 REPLACE가 지울 때는
-- 지우기 막기 트리거가 돌지 않으므로(recursive_triggers 꺼짐) 키가 겹치는 넣기를 스스로 거부한다.
CREATE TRIGGER trades_no_replace BEFORE INSERT ON trades
WHEN EXISTS (SELECT 1 FROM trades WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'trades rows are kept'); END;
CREATE TRIGGER legacy_boundary_no_replace BEFORE INSERT ON legacy_boundary
WHEN EXISTS (SELECT 1 FROM legacy_boundary WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'legacy_boundary is fixed'); END;
CREATE TRIGGER template_revisions_no_replace BEFORE INSERT ON template_revisions
WHEN EXISTS (SELECT 1 FROM template_revisions
             WHERE template_id = NEW.template_id AND revision = NEW.revision)
BEGIN SELECT RAISE(ABORT, 'template_revisions is append-only'); END;
CREATE TRIGGER key_hashes_no_replace BEFORE INSERT ON key_hashes
WHEN EXISTS (SELECT 1 FROM key_hashes WHERE generation = NEW.generation)
BEGIN SELECT RAISE(ABORT, 'key_hashes is append-only'); END;
CREATE TRIGGER agent_sessions_no_replace BEFORE INSERT ON agent_sessions
WHEN EXISTS (SELECT 1 FROM agent_sessions WHERE id = NEW.id)
  OR (NEW.ended_at IS NULL AND EXISTS (SELECT 1 FROM agent_sessions
      WHERE agent = NEW.agent AND title = NEW.title AND ended_at IS NULL))
BEGIN SELECT RAISE(ABORT, 'agent_sessions rows are kept'); END;
CREATE TRIGGER room_events_no_replace BEFORE INSERT ON room_events
WHEN EXISTS (SELECT 1 FROM room_events
             WHERE room = NEW.room AND (seq = NEW.seq OR event_id = NEW.event_id))
BEGIN SELECT RAISE(ABORT, 'room_events is append-only'); END;
CREATE TRIGGER trigger_receipts_no_replace BEFORE INSERT ON trigger_receipts
WHEN EXISTS (SELECT 1 FROM trigger_receipts
             WHERE job = NEW.job AND (slot = NEW.slot OR status = 'running'))
BEGIN SELECT RAISE(ABORT, 'trigger_receipts rows are kept'); END;
";

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    fn fresh() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi.sqlite");
        drop(super::super::open(&path).unwrap());
        (dir, path)
    }

    #[test]
    fn 방_로그는_sql로도_번호를_건너뛰거나_표에_없는_짝을_쓸_수_없다() {
        let (_dir, path) = fresh();
        let conn = Connection::open(&path).unwrap();
        let insert = "INSERT INTO room_events (room, seq, event_id, kind, actor_kind, actor_id, \
                      payload, at) VALUES ('팀', ?1, ?2, ?3, ?4, 'a', '{}', 1)";
        conn.execute(insert, rusqlite::params![1, "e1", "message.user", "user"])
            .unwrap();
        for (seq, id, kind, actor) in [
            (3, "e3", "message.user", "user"),
            (1, "e1b", "message.user", "user"),
            (2, "e2", "message.user", "member"),
        ] {
            assert!(
                conn.execute(insert, rusqlite::params![seq, id, kind, actor])
                    .is_err()
            );
        }
        assert!(
            conn.execute("INSERT INTO room_event_kinds VALUES ('user', 'chat')", [])
                .is_err()
        );
        assert!(conn.execute("DELETE FROM room_events", []).is_err());
    }

    #[test]
    fn 근거가_붙은_매매의_판단_시각을_자료보다_앞으로_옮길_수_없다() {
        let (_dir, path) = fresh();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO trades (id, symbol, account, state, created_at, updated_at, decided_at) \
             VALUES (1, '005930', 'paper', 'judged', 0, 0, 100);
             INSERT INTO facts (id, kind, subject, label, value_kind, value, source_name, \
             source_url, known_at) VALUES (1, 'k', 's', 'l', 'number', '1', 'n', 'u', 90);
             INSERT INTO trade_facts (trade_id, fact_id) VALUES (1, 1);",
        )
        .unwrap();
        assert!(
            conn.execute("UPDATE trades SET decided_at = 80 WHERE id = 1", [])
                .is_err()
        );
        assert!(
            conn.execute("UPDATE trades SET decided_at = NULL WHERE id = 1", [])
                .is_err()
        );
        conn.execute("UPDATE trades SET decided_at = 95 WHERE id = 1", [])
            .unwrap();
        assert!(conn.execute("DELETE FROM trade_facts", []).is_err());
    }

    #[test]
    fn 닫힌_영수증은_sql로도_다시_실행_중이_되지_않는다() {
        let (_dir, path) = fresh();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO trigger_receipts (job, slot, status, delivery, started_at) \
             VALUES ('j', 1, 'running', 'not_started', 1);
             UPDATE trigger_receipts SET status = 'ok', finished_at = 2;",
        )
        .unwrap();
        assert!(
            conn.execute(
                "UPDATE trigger_receipts SET status = 'running', finished_at = NULL",
                []
            )
            .is_err()
        );
        assert!(
            conn.execute(
                "INSERT INTO trigger_receipts (job, slot, status, delivery, started_at, \
                 finished_at) VALUES ('j', 2, 'ok', 'not_started', 1, 2)",
                []
            )
            .is_err()
        );
    }
}
