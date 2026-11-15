#![cfg(test)]
//! 7차 (접속 관문 7차 6번을 받친다): 옛 봉투 줄의 번호([`Store::legacy_envelope_seqs`])와
//! 시각 칸에 실수가 적힌 줄 읽기.
//!
//! - 1번: 경계 번호 이하의 줄 가운데 옛 봉투 줄의 번호만 준다. 규칙은 [`Store::legacy_envelope_until`]
//!   문서와 같다(정확히 두 칸, 맨 위 칸 이름이 겹치지 않음, `at`이 i64 범위 안의 정수이고 `-0`이 아님,
//!   짝 없는 서로게이트 없음, 시각 칸이 비었거나 내용의 `at`과 같음). 판정은 저장된 원래 글자로 한다.
//! - 2번: 버전 2 파일에서 `at`이 i64 범위 밖인 봉투 줄은 버전 3으로 올라오며 시각 칸에 실수가 적힌다.
//!   그 줄은 시각 없음으로 읽고, 뒤의 이벤트도 읽힌다.
//!
//! 픽스처는 바꾸지 않고 임시 폴더에 복사해 쓴다(`legacy_boundary.rs` 머리말):
//! `v1.db`(봉투 없음, 이벤트 3개) · `v2.db`(봉투 모양 2 · 3 · 7번, 이벤트 7개) ·
//! `v3.db`(옛 봉투 줄 2 · 3 · 7 · 10번, 이벤트 11개) · `v3_plain.db`(옛 봉투 줄 3번, 이벤트 4개).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::{t, temp_store};
use pi_store::{EventRange, Store};
use serde_json::{Value, json};

const FIXTURE_V1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v1.db");
const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2.db");
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3.db");
const FIXTURE_V3_PLAIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3_plain.db");

/// 픽스처를 임시 폴더에 복사한다. 원본은 건드리지 않는다.
fn copy_of(fixture: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    fs::copy(fixture, &path).unwrap();
    (dir, path)
}

fn raw_user_version(path: &Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

/// 버전 3 파일에 이벤트 줄을 저장소를 거치지 않고 넣는다: (번호, 시각 칸, 내용 글자).
fn raw_insert_v3(path: &Path, rows: &[(i64, Option<i64>, &str)]) {
    let conn = rusqlite::Connection::open(path).unwrap();
    for (seq, at, payload) in rows {
        conn.execute(
            "INSERT INTO events (seq, kind, payload, at) VALUES (?1, 'feed_status', ?2, ?3)",
            rusqlite::params![seq, payload, at],
        )
        .unwrap();
    }
}

/// 버전 2 파일에 이벤트 줄을 저장소를 거치지 않고 넣는다(버전 2에는 시각 칸이 없다).
fn raw_insert_v2(path: &Path, rows: &[(i64, &str)]) {
    let conn = rusqlite::Connection::open(path).unwrap();
    for (seq, payload) in rows {
        conn.execute(
            "INSERT INTO events (seq, kind, payload) VALUES (?1, 'feed_status', ?2)",
            rusqlite::params![seq, payload],
        )
        .unwrap();
    }
}

/// 시각 칸에 적힌 값의 SQLite 형(`integer` · `real` · `null`)을 저장소를 거치지 않고 읽는다.
fn raw_at_type(path: &Path, seq: i64) -> String {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row(
        "SELECT typeof(at) FROM events WHERE seq = ?1",
        [seq],
        |row| row.get(0),
    )
    .unwrap()
}

fn all_seqs(store: &Store) -> Vec<u64> {
    store.legacy_envelope_seqs(0, usize::MAX).unwrap()
}

// ───────────── 경계가 없으면 빈 목록 ─────────────

// 검증: D6, V6, C2, E3
#[test]
fn 새로_만든_파일은_경계가_없어_봉투_모양_줄이_있어도_빈_목록이다() {
    let (_dir, path, store) = temp_store();
    assert_eq!(store.legacy_envelope_until().unwrap(), None);
    assert_eq!(all_seqs(&store), Vec::<u64>::new());

    // 봉투 모양 내용을 시각 없이 · 내용의 `at`과 같은 시각으로 쌓아도 옛 봉투가 아니다.
    let envelope = json!({ "at": t(10).0, "payload": { "ok": true } });
    store.append("feed_status", &envelope).unwrap();
    store.append_at("feed_status", &envelope, t(10)).unwrap();
    store
        .append("chat", &json!({ "at": 1, "payload": null }))
        .unwrap();
    assert_eq!(all_seqs(&store), Vec::<u64>::new());
    for (after, max) in [(0, 0), (0, 1), (1, 10), (u64::MAX, 10)] {
        assert_eq!(
            store.legacy_envelope_seqs(after, max).unwrap(),
            Vec::<u64>::new(),
            "after={after} max={max}"
        );
    }

    // 다시 열어도 같다.
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), None);
    assert_eq!(all_seqs(&store), Vec::<u64>::new());
}

// ───────────── 픽스처의 옛 봉투 줄 ─────────────

// 검증: D6, V6, C2, E3
#[test]
fn 픽스처를_올리면_경계_이하의_옛_봉투_줄만_나온다() {
    // (픽스처, 경계, 옛 봉투 줄)
    for (fixture, until, want) in [
        (FIXTURE_V1, Some(3), vec![]),
        (FIXTURE_V2, Some(7), vec![2, 3, 7]),
        (FIXTURE_V3, Some(10), vec![2, 3, 7, 10]),
        (FIXTURE_V3_PLAIN, Some(3), vec![3]),
    ] {
        let (_dir, path) = copy_of(fixture);
        let store = Store::open(&path).unwrap();
        assert_eq!(store.legacy_envelope_until().unwrap(), until, "{fixture}");
        assert_eq!(all_seqs(&store), want, "{fixture}");
        // 다시 열어도 같다.
        drop(store);
        let store = Store::open(&path).unwrap();
        assert_eq!(all_seqs(&store), want, "다시 열기: {fixture}");
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 경계_뒤에_쌓은_봉투_모양_줄은_나오지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));

    // 12번부터: 시각 없음 · 내용의 `at`과 같은 시각 · 다른 시각, 모두 봉투 모양.
    let envelope = json!({ "at": t(3000).0, "payload": { "ok": true } });
    assert_eq!(store.append("feed_status", &envelope).unwrap(), 12);
    assert_eq!(
        store.append_at("feed_status", &envelope, t(3000)).unwrap(),
        13
    );
    assert_eq!(
        store.append_at("feed_status", &envelope, t(3001)).unwrap(),
        14
    );
    assert_eq!(
        store
            .append("chat", &json!({ "at": 0, "payload": null }))
            .unwrap(),
        15
    );

    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    assert_eq!(all_seqs(&store), vec![2, 3, 7, 10]);
    // 경계 뒤를 가리키는 `after`에서는 아무것도 나오지 않는다.
    for after in [10, 11, 12, 14, 15, 100] {
        assert_eq!(
            store.legacy_envelope_seqs(after, usize::MAX).unwrap(),
            Vec::<u64>::new(),
            "after={after}"
        );
    }
    // 다시 열어도 같다.
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(all_seqs(&store), vec![2, 3, 7, 10]);
}

// ───────────── 원래 글자로 가르는 규칙 ─────────────

/// 손으로 만든 줄: (내용 글자, 시각 칸, 옛 봉투인가). `v3.db`의 12번부터 순서대로 넣는다.
fn handmade_rows() -> Vec<(&'static str, Option<i64>, bool)> {
    vec![
        // 맨 위 칸 이름이 겹친 줄. 읽힌 JSON 값으로는 `{"at":1,"payload":…}`처럼 보이지만 옛 봉투가 아니다.
        (r#"{"at":1,"at":1,"payload":1}"#, None, false),
        (r#"{"at":1,"at":1,"payload":1}"#, Some(1), false),
        (r#"{"at":1,"payload":1,"at":1}"#, None, false),
        (r#"{"at":2,"at":1,"payload":1}"#, Some(1), false),
        (r#"{"at":1,"payload":1,"payload":2}"#, None, false),
        (r#"{"at":1,"payload":1,"payload":2}"#, Some(1), false),
        (r#"{"payload":1,"at":1,"payload":1}"#, None, false),
        (r#"{"at":1,"x":0,"payload":1}"#, None, false),
        (r#"{"at":1,"payload":1,"x":0,"x":1}"#, None, false),
        // 정확히 두 칸이 아닌 줄
        (r#"{"at":1,"payload":1,"x":2}"#, None, false),
        (r#"{"at":1,"payload":1,"x":2}"#, Some(1), false),
        (r#"{"at":1}"#, None, false),
        (r#"{"at":1,"pay":1}"#, None, false),
        (r#"{}"#, None, false),
        // `at`이 실수 모양 · `-0` · i64 범위 밖 · 정수가 아님
        (r#"{"at":1.0,"payload":1}"#, None, false),
        (r#"{"at":1.0,"payload":1}"#, Some(1), false),
        (r#"{"at":1e3,"payload":1}"#, None, false),
        (r#"{"at":1E0,"payload":1}"#, Some(1), false),
        (r#"{"at":1.5,"payload":1}"#, None, false),
        (r#"{"at":-0,"payload":1}"#, None, false),
        (r#"{"at":-0,"payload":1}"#, Some(0), false),
        (r#"{"at":9223372036854775808,"payload":1}"#, None, false),
        (r#"{"at":-9223372036854775809,"payload":1}"#, None, false),
        (r#"{"at":18446744073709551616,"payload":1}"#, None, false),
        (r#"{"at":"1","payload":1}"#, None, false),
        (r#"{"at":null,"payload":1}"#, None, false),
        (r#"{"at":true,"payload":1}"#, None, false),
        // 짝 없는 서로게이트 이스케이프
        (r#"{"at":1,"payload":"\ud800"}"#, None, false),
        (r#"{"at":1,"payload":"\udc00"}"#, Some(1), false),
        (r#"{"at":1,"payload":{"k":"a\ud800b"}}"#, None, false),
        // 시각 칸이 내용의 `at`과 다른 줄
        (r#"{"at":1,"payload":1}"#, Some(2), false),
        (r#"{"at":0,"payload":1}"#, Some(1), false),
        // JSON이 아니거나 객체가 아닌 내용
        ("JSON 아님", None, false),
        (r#"[1,2]"#, None, false),
        (r#""at""#, None, false),
        ("null", None, false),
        // 옛 봉투 줄(대조): 두 칸 순서 · 공백 · 0 · i64 양 끝 · 짝이 맞는 서로게이트 · 백슬래시 이스케이프
        (r#"{"at":1,"payload":1}"#, None, true),
        (r#"{"at":1,"payload":1}"#, Some(1), true),
        (r#"{"payload":1,"at":1}"#, None, true),
        (
            r#"{ "at" : 5 , "payload" : [1, {"at": 2}] }"#,
            Some(5),
            true,
        ),
        (r#"{"at":0,"payload":null}"#, None, true),
        (r#"{"at":0,"payload":null}"#, Some(0), true),
        (r#"{"at":-1,"payload":{}}"#, Some(-1), true),
        (r#"{"at":9223372036854775807,"payload":1}"#, None, true),
        (
            r#"{"at":-9223372036854775808,"payload":1}"#,
            Some(i64::MIN),
            true,
        ),
        (r#"{"at":1,"payload":"😀"}"#, None, true),
        (r#"{"at":1,"payload":"\\ud800"}"#, Some(1), true),
        (r#"{"at":1,"payload":{"at":1,"at":2}}"#, None, true),
        // 맨 끝은 옛 봉투 줄이라 경계가 여기까지 온다.
        (r#"{"at":7,"payload":"끝"}"#, None, true),
    ]
}

/// `v3.db`를 복사해 손으로 만든 줄을 12번부터 넣고 연다. 돌려주는 것: (임시 폴더, 저장소, 기대 번호).
fn store_with_handmade() -> (tempfile::TempDir, Store, Vec<u64>) {
    let (dir, path) = copy_of(FIXTURE_V3);
    let rows = handmade_rows();
    let mut raw = Vec::new();
    let mut want = vec![2, 3, 7, 10];
    for (i, (text, at, legacy)) in rows.iter().enumerate() {
        let seq = 12 + i as i64;
        raw.push((seq, *at, *text));
        if *legacy {
            want.push(seq as u64);
        }
    }
    raw_insert_v3(&path, &raw);
    assert_eq!(raw_user_version(&path), 3);
    let store = Store::open(&path).unwrap();
    let last = 12 + rows.len() as u64 - 1;
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(last));
    (dir, store, want)
}

// 검증: D6, V6, C2, E3
#[test]
fn 맨_위_칸_이름이_겹친_줄은_경계_안에_있어도_나오지_않는다() {
    let (_dir, store, want) = store_with_handmade();
    let got = all_seqs(&store);
    assert_eq!(got, want);
    // 겹친 이름 줄(12~20번)은 하나도 나오지 않는다.
    for seq in 12..=20 {
        assert!(!got.contains(&seq), "겹친 이름 줄이 나왔다: {seq}");
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 손으로_만든_줄은_하나씩_규칙대로_가른다() {
    let (_dir, store, _) = store_with_handmade();
    let got = all_seqs(&store);
    for (i, (text, at, legacy)) in handmade_rows().into_iter().enumerate() {
        let seq = 12 + i as u64;
        assert_eq!(
            got.contains(&seq),
            legacy,
            "{seq}번 내용={text} 시각 칸={at:?}"
        );
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 옛_봉투_줄은_경계_번호와_같은_규칙이다() {
    // 겹친 이름 · 실수 모양 · -0 · 범위 밖 · 서로게이트 줄만 경계 위에 두면 경계는 그 아래 옛 봉투 줄에서 멈춘다.
    for (text, at, legacy) in handmade_rows() {
        let (_dir, path) = copy_of(FIXTURE_V3);
        raw_insert_v3(&path, &[(12, at, text)]);
        let store = Store::open(&path).unwrap();
        let (until, want) = if legacy {
            (Some(12), vec![2, 3, 7, 10, 12])
        } else {
            (Some(10), vec![2, 3, 7, 10])
        };
        assert_eq!(
            store.legacy_envelope_until().unwrap(),
            until,
            "경계: {text} {at:?}"
        );
        assert_eq!(all_seqs(&store), want, "번호: {text} {at:?}");
    }
}

// ───────────── 나눠 읽기 ─────────────

// 검증: D6, V6, C2, E3
#[test]
fn after와_max로_나눠_읽는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    for (after, max, want) in [
        (0, usize::MAX, vec![2, 3, 7, 10]),
        (0, 4, vec![2, 3, 7, 10]),
        (0, 2, vec![2, 3]),
        (0, 1, vec![2]),
        (0, 0, vec![]),
        // `after`는 그 번호를 빼고 뒤부터다.
        (2, 2, vec![3, 7]),
        (3, 2, vec![7, 10]),
        (1, usize::MAX, vec![2, 3, 7, 10]),
        (4, usize::MAX, vec![7, 10]),
        (7, 1, vec![10]),
        (9, usize::MAX, vec![10]),
        (10, usize::MAX, vec![]),
        (11, usize::MAX, vec![]),
        (u64::MAX, usize::MAX, vec![]),
        (5, 0, vec![]),
    ] {
        assert_eq!(
            store.legacy_envelope_seqs(after, max).unwrap(),
            want,
            "after={after} max={max}"
        );
    }

    // 쪽마다 마지막 번호를 다음 `after`로 넘기면 빠짐 · 겹침 없이 전체가 나온다.
    let mut pages = Vec::new();
    let mut after = 0;
    loop {
        let page = store.legacy_envelope_seqs(after, 1).unwrap();
        let Some(&last) = page.last() else { break };
        pages.extend(page);
        after = last;
    }
    assert_eq!(pages, all_seqs(&store));
}

// 검증: D6, V6, C2, E3
#[test]
fn 손으로_만든_줄도_나눠_읽으면_전체와_같다() {
    let (_dir, store, want) = store_with_handmade();
    for size in [1, 2, 3, 5] {
        let mut got = Vec::new();
        let mut after = 0;
        loop {
            let page = store.legacy_envelope_seqs(after, size).unwrap();
            assert!(page.len() <= size);
            let Some(&last) = page.last() else { break };
            assert!(page.windows(2).all(|w| w[0] < w[1]), "번호 순서가 아니다");
            assert!(page[0] > after);
            got.extend(page);
            after = last;
        }
        assert_eq!(got, want, "size={size}");
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 옛_봉투_번호를_읽어도_아무것도_바뀌지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    let before = store
        .read_events(EventRange::default(), usize::MAX)
        .unwrap();
    for _ in 0..3 {
        assert_eq!(all_seqs(&store), vec![2, 3, 7, 10]);
    }
    assert_eq!(
        store
            .read_events(EventRange::default(), usize::MAX)
            .unwrap(),
        before
    );
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    // 다음 번호도 그대로 이어진다.
    assert_eq!(store.append("chat", &json!({ "n": 12 })).unwrap(), 12);
}

// ───────────── 2번: 시각 칸에 실수가 적힌 줄 ─────────────

/// `v2.db`를 복사해 8번부터 줄을 넣는다. 8번은 `at`이 i64 범위 밖(1e20 정수)인 봉투 줄이다.
fn v2_with_out_of_range_at() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = copy_of(FIXTURE_V2);
    assert_eq!(raw_user_version(&path), 2);
    raw_insert_v2(
        &path,
        &[
            (8, r#"{"at":100000000000000000000,"payload":{"ok":true}}"#),
            (9, r#"{"at":-100000000000000000000,"payload":1}"#),
            (10, r#"{"at":-0,"payload":2}"#),
            (11, r#"{"at":1800000003000,"payload":{"n":11}}"#),
        ],
    );
    (dir, path)
}

// 검증: D6, V6, C2, E3
#[test]
fn 시각_칸에_실수가_적힌_줄은_시각_없음으로_읽고_뒤의_이벤트도_읽힌다() {
    let (_dir, path) = v2_with_out_of_range_at();
    let store = Store::open(&path).unwrap();
    assert_eq!(store.info().unwrap().schema_version, 4);
    // 버전 3으로 올라오며 범위 밖 정수는 시각 칸에 실수로 적혔다(SQLite가 정수로 담지 못한다).
    assert_eq!(raw_at_type(&path, 8), "real");
    assert_eq!(raw_at_type(&path, 9), "real");

    let events = store
        .read_events(EventRange::default(), usize::MAX)
        .unwrap();
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, (1..=11).collect::<Vec<u64>>());

    let eighth = &events[7];
    assert_eq!(eighth.kind, "feed_status");
    assert_eq!(eighth.at, None);
    assert_eq!(eighth.payload["payload"], json!({ "ok": true }));
    assert_eq!(events[8].at, None);
    assert_eq!(events[8].payload["payload"], json!(1));
    // `-0`은 정수 0으로 옮겨 적혔다(시각 칸은 정수라 그대로 읽는다).
    assert_eq!(events[9].payload["payload"], json!(2));
    // 뒤의 줄은 시각까지 그대로 읽힌다.
    let eleventh = &events[10];
    assert_eq!(eleventh.at, Some(t(3000)));
    assert_eq!(
        eleventh.payload,
        json!({ "at": t(3000).0, "payload": { "n": 11 } })
    );

    // 범위를 나눠 읽어도 실패하지 않는다.
    let after_seven = store
        .read_events(
            EventRange {
                after_seq: 7,
                ..EventRange::default()
            },
            2,
        )
        .unwrap();
    assert_eq!(
        after_seven
            .iter()
            .map(|e| (e.seq, e.at))
            .collect::<Vec<_>>(),
        vec![(8, None), (9, None)]
    );
    // 시각 조건을 주면 시각 없는 줄로 빠진다.
    let timed = store
        .read_events(
            EventRange {
                after_at: Some(t(2999)),
                ..EventRange::default()
            },
            usize::MAX,
        )
        .unwrap();
    assert_eq!(timed.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![11]);

    // 다시 열어도 같다.
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .read_events(EventRange::default(), usize::MAX)
            .unwrap(),
        events
    );
}

// 검증: D6, V6, C2, E3
#[test]
fn 시각_칸에_실수가_적힌_줄은_옛_봉투_줄이_아니다() {
    let (_dir, path) = v2_with_out_of_range_at();
    let store = Store::open(&path).unwrap();
    // 버전 2 파일의 경계는 그 시점의 마지막 번호다.
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(11));
    // 8 · 9번(범위 밖) · 10번(`-0`)은 빠지고 11번은 든다.
    assert_eq!(all_seqs(&store), vec![2, 3, 7, 11]);
    assert_eq!(store.legacy_envelope_seqs(7, 10).unwrap(), vec![11]);
    let payload: Value = store
        .read_events(EventRange::default(), usize::MAX)
        .unwrap()[7]
        .payload
        .clone();
    assert!(payload.is_object());
}

// ───────────── 필수 번호 (지시서 `## 필수 번호`) ─────────────

// 검증: M1, M2, M5, M6, E12
#[test]
fn 옛_봉투_번호는_다른_기록에_기대지_않는다() {
    // 팀 상태 · 매매 · 접속 기록 표가 차 있어도 옛 봉투 번호는 이벤트 줄로만 정한다.
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    let before_team = store.team_history().unwrap();
    let before_access = store.access_log().unwrap();
    assert_eq!(all_seqs(&store), vec![2, 3, 7, 10]);
    assert_eq!(store.team_history().unwrap(), before_team);
    assert_eq!(store.access_log().unwrap(), before_access);
}
