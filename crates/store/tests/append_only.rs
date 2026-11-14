#![cfg(test)]
//! 2차 규칙 4: 기록 표(투자 팀 기록 · 이벤트 · 접속 기록)는 어떤 SQL로도 바뀌지 않는다.
//!
//! 공개 API에는 기록을 고치는 길이 없어서 공개 API만으로는 확인할 수 없다.
//! `open.rs`처럼 저장소 파일을 `rusqlite`로 직접 열어 SQL을 넣어 본 뒤, 공개 API로 다시 읽어 견준다.

mod common;

use std::path::Path;

use common::{t, temp_store};
use pi_store::{
    AccessRecord, Event, NewAccess, Store, TeamCause, TeamChange, TeamState, TeamStateRecord,
};
use serde_json::json;

/// 공개 API로 읽은 기록 전체.
#[derive(Debug, PartialEq)]
struct Records {
    team_state: TeamState,
    team_history: Vec<TeamStateRecord>,
    events: Vec<Event>,
    access_log: Vec<AccessRecord>,
}

fn read_records(path: &Path) -> Records {
    let store = Store::open(path).unwrap();
    Records {
        team_state: store.team_state().unwrap(),
        team_history: store.team_history().unwrap(),
        events: store.read_after(0, 1_000).unwrap(),
        access_log: store.access_log().unwrap(),
    }
}

/// 기록 표마다 세 줄씩 넣는다.
fn fill(store: &Store) {
    for (i, (to, cause)) in [
        (TeamState::Operating, TeamCause::TradingStart),
        (TeamState::Resting, TeamCause::MarketClose),
        (TeamState::Operating, TeamCause::TradingStart),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .change_team_state(TeamChange {
                to,
                at: t(i as i64),
                cause,
                detail: format!("{i}번째 변경"),
                owner_approved: false,
            })
            .unwrap();
    }
    for n in 1..=3 {
        store
            .append("chat", &json!({ "n": n, "text": "원래 내용" }))
            .unwrap();
        store
            .record_access(NewAccess {
                at: t(100 + n),
                origin: "127.0.0.1".to_string(),
                action: format!("{n}번째 접속"),
            })
            .unwrap();
    }
}

/// 기록이 든 파일을 만들고 닫는다. 넣기 전의 모습을 함께 준다.
fn filled_file() -> (tempfile::TempDir, std::path::PathBuf, Records) {
    let (dir, path, store) = temp_store();
    fill(&store);
    drop(store);
    let before = read_records(&path);
    assert_eq!(before.team_history.len(), 3);
    assert_eq!(before.events.len(), 3);
    assert_eq!(before.access_log.len(), 3);
    (dir, path, before)
}

/// SQL을 파일에 직접 넣어 본다. 저장소가 막아서 거절되거나(제약 오류) 조용히 무시되는 것은 괜찮다.
/// 그 밖의 오류는 시험하는 SQL 자체가 틀린 것이므로 테스트를 멈춘다.
/// 실제로 바뀌지 않았는지는 부르는 쪽이 공개 API로 다시 읽어 본다.
fn try_sql(path: &Path, sql: &str) {
    let conn = rusqlite::Connection::open(path).unwrap();
    match conn.execute(sql, []) {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation => {}
        Err(other) => panic!("시험하는 SQL이 틀렸다: {sql}\n{other}"),
    }
}

// 검증: M1, C2, E3, M6
#[test]
fn 기록_표는_고치기와_지우기_sql로_바뀌지_않는다() {
    let (_dir, path, before) = filled_file();
    for sql in [
        // 투자 팀 기록
        "UPDATE team_history SET detail = '바꿔치기', owner_approved = 1 WHERE seq = 1",
        "UPDATE team_history SET seq = 99 WHERE seq = 3",
        "DELETE FROM team_history WHERE seq = 3",
        "DELETE FROM team_history",
        // 이벤트
        "UPDATE events SET payload = '{\"forged\":true}' WHERE seq = 2",
        "UPDATE events SET seq = seq + 10",
        "UPDATE OR REPLACE events SET seq = 1 WHERE seq = 2",
        "DELETE FROM events WHERE seq = 1",
        "DELETE FROM events",
        "INSERT INTO events (seq, kind, payload) VALUES (1, 'forged', '{}')
             ON CONFLICT (seq) DO UPDATE SET kind = excluded.kind, payload = excluded.payload",
        // 접속 기록
        "UPDATE access_log SET origin = '10.0.0.1', action = '바꿔치기' WHERE seq = 1",
        "DELETE FROM access_log WHERE seq = 2",
        "DELETE FROM access_log",
        "INSERT INTO access_log (seq, at, origin, action) VALUES (3, 0, 'forged', 'forged')
             ON CONFLICT (seq) DO UPDATE SET origin = excluded.origin",
    ] {
        try_sql(&path, sql);
        assert_eq!(read_records(&path), before, "바뀌었다: {sql}");
    }
}

// 검증: M1
#[test]
fn 투자_팀_기록은_같은_번호로_다시_넣어도_바뀌지_않는다() {
    let (_dir, path, before) = filled_file();
    for sql in [
        // 첫 줄의 설명과 승인 표시를 바꿔치기.
        "INSERT OR REPLACE INTO team_history
             (seq, at, from_state, to_state, cause, detail, owner_approved)
         SELECT seq, at, from_state, to_state, cause, '바꿔치기', 1
         FROM team_history WHERE seq = 1",
        // 마지막 줄을 앞줄의 내용으로 덮어 지금 상태를 바꿔치기(운용 중 → 쉬는 중).
        "REPLACE INTO team_history
             (seq, at, from_state, to_state, cause, detail, owner_approved)
         SELECT 3, at, from_state, to_state, cause, detail, owner_approved
         FROM team_history WHERE seq = 2",
    ] {
        try_sql(&path, sql);
        assert_eq!(read_records(&path), before, "바뀌었다: {sql}");
    }
    assert_eq!(before.team_state, TeamState::Operating);
}

// 검증: C2, E3
#[test]
fn 이벤트는_같은_번호로_다시_넣어도_바뀌지_않는다() {
    let (_dir, path, before) = filled_file();
    for sql in [
        "INSERT OR REPLACE INTO events (seq, kind, payload) VALUES (1, 'forged', '{\"forged\":true}')",
        "REPLACE INTO events (seq, kind, payload) VALUES (3, 'chat', '{\"n\":3,\"text\":\"고친 내용\"}')",
        "INSERT OR REPLACE INTO events (rowid, kind, payload) VALUES (2, 'forged', '{}')",
    ] {
        try_sql(&path, sql);
        assert_eq!(read_records(&path), before, "바뀌었다: {sql}");
    }
}

// 검증: M6
#[test]
fn 접속_기록은_같은_번호로_다시_넣어도_바뀌지_않는다() {
    let (_dir, path, before) = filled_file();
    for sql in [
        "INSERT OR REPLACE INTO access_log (seq, at, origin, action) VALUES (1, 0, '10.0.0.1', '바꿔치기')",
        "REPLACE INTO access_log (seq, at, origin, action) VALUES (3, 0, '10.0.0.1', '바꿔치기')",
    ] {
        try_sql(&path, sql);
        assert_eq!(read_records(&path), before, "바뀌었다: {sql}");
    }
}

// 검증: C2, E3, M6, M1
#[test]
fn 막힌_뒤에도_새_기록은_번호를_이어_쌓인다() {
    let (_dir, path, before) = filled_file();
    try_sql(
        &path,
        "INSERT OR REPLACE INTO events (seq, kind, payload) VALUES (3, 'forged', '{}')",
    );
    try_sql(
        &path,
        "INSERT OR REPLACE INTO access_log (seq, at, origin, action) VALUES (3, 0, 'forged', 'forged')",
    );
    assert_eq!(read_records(&path), before);

    // 바뀌지 않게 막는 장치가 정상 쌓기까지 막지는 않는다.
    let store = Store::open(&path).unwrap();
    assert_eq!(store.append("chat", &json!({ "n": 4 })).unwrap(), 4);
    assert_eq!(
        store
            .record_access(NewAccess {
                at: t(200),
                origin: "127.0.0.1".to_string(),
                action: "네 번째 접속".to_string(),
            })
            .unwrap(),
        4
    );
    let rec = store
        .change_team_state(TeamChange {
            to: TeamState::Resting,
            at: t(300),
            cause: TeamCause::MarketClose,
            detail: "장 마감".to_string(),
            owner_approved: false,
        })
        .unwrap();
    assert_eq!(rec.seq, 4);
    assert_eq!(store.read_after(0, 10).unwrap()[..3], before.events[..]);
}
