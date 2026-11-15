#![cfg(test)]
//! 8차: 옛 봉투 판정의 이스케이프 칸 이름과 위 끝을 준 옛 봉투 번호.
//!
//! - 1번: 칸 이름에 이스케이프가 든 줄(`{"at":1,"payload":2}` 등)도 접속 관문의 `serde_json`이
//!   읽는 이름(이스케이프를 푼 이름)으로 가른다. 겹침도 푼 이름으로 가른다. 경계
//!   ([`Store::legacy_envelope_until`])와 목록([`Store::legacy_envelope_seqs`])이 함께 따른다.
//! - 2번: [`Store::legacy_envelope_seqs_upto`]는 `after`보다 크고 `upto` 이하인 옛 봉투 줄 번호만
//!   준다(경계 뒤는 늘 빠진다). 기존 [`Store::legacy_envelope_seqs`]는 그대로다.
//!
//! 픽스처는 바꾸지 않고 임시 폴더에 복사해 쓴다: `v2.db`(봉투 모양 2 · 3 · 7번, 이벤트 7개) ·
//! `v3.db`(옛 봉투 줄 2 · 3 · 7 · 10번, 이벤트 11개). 줄은 저장소를 거치지 않고 rusqlite로 넣는다.

use std::fs;
use std::path::{Path, PathBuf};

use pi_store::{EventRange, Store};
use serde_json::json;

const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v2.db");
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/v3.db");

/// `v3.db`의 옛 봉투 줄.
const V3_LEGACY: [u64; 4] = [2, 3, 7, 10];

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

fn all_seqs(store: &Store) -> Vec<u64> {
    store.legacy_envelope_seqs(0, usize::MAX).unwrap()
}

// ───────────── 1번: 이스케이프가 든 칸 이름 ─────────────

/// 손으로 만든 줄: (내용 글자, 시각 칸, 옛 봉투인가). 이름은 `serde_json`이 이스케이프를 풀어 읽는
/// 이름으로 본다.
fn escape_rows() -> Vec<(&'static str, Option<i64>, bool)> {
    vec![
        // 푼 이름이 `at` · `payload`인 두 칸 객체: 옛 봉투 줄이다.
        (r#"{"at":1,"payload":1}"#, None, true),
        (r#"{"at":1,"payload":1}"#, Some(1), true),
        (r#"{"at":2,"payload":"x"}"#, Some(2), true),
        (r#"{"at":3,"payload":null}"#, None, true),
        (r#"{"at":4,"payload":{}}"#, None, true),
        (r#"{"at":4,"payload":[1]}"#, Some(4), true),
        (r#"{"payload":5,"at":5}"#, Some(5), true),
        (
            r#"{ "at" : -6 , "payload" : {"at": 1, "at": 2} }"#,
            None,
            true,
        ),
        // 이름이 겹친 줄: 겹침이 이스케이프를 풀어야 드러난다.
        (r#"{"at":1,"at":2,"payload":3}"#, None, false),
        (r#"{"at":1,"at":2,"payload":3}"#, None, false),
        (r#"{"at":1,"at":2,"payload":3}"#, Some(1), false),
        (r#"{"at":1,"at":2,"payload":3}"#, Some(2), false),
        (r#"{"at":1,"at":1,"payload":1}"#, Some(1), false),
        (r#"{"at":1,"at":1,"payload":1}"#, None, false),
        (r#"{"at":1,"payload":1,"payload":2}"#, None, false),
        (r#"{"payload":1,"at":1,"payload":1}"#, Some(1), false),
        // 푼 이름이 `at` · `payload`가 아닌 줄
        (r#"{"bt":1,"payload":1}"#, None, false),
        (r#"{"At":1,"payload":1}"#, None, false),
        (r#"{"aT":1,"payload":1}"#, None, false),
        (r#"{"a\\u0074":1,"payload":1}"#, None, false),
        (r#"{"\/at":1,"payload":1}"#, None, false),
        (r#"{"at\u0000":1,"payload":1}"#, None, false),
        (r#"{"at":1,"payload ":1}"#, None, false),
        (r#"{"😀":1,"payload":1}"#, None, false),
        (r#"{"\ud800t":1,"payload":1}"#, None, false),
        // 이스케이프 이름이어도 나머지 규칙은 그대로다.
        (r#"{"at":1,"payload":1,"x":2}"#, None, false),
        (r#"{"at":1,"payload":1}"#, Some(2), false),
        (r#"{"at":1.0,"payload":1}"#, None, false),
        (r#"{"at":-0,"payload":1}"#, None, false),
        (r#"{"at":9223372036854775808,"payload":1}"#, None, false),
        (r#"{"at":"1","payload":1}"#, None, false),
        (r#"{"at":1,"payload":"\ud800"}"#, None, false),
        (r#"{"\u006":1,"payload":1}"#, None, false),
        // 맨 끝은 옛 봉투 줄이라 경계가 여기까지 온다.
        (r#"{"at":7,"payload":"끝"}"#, None, true),
    ]
}

/// `v3.db`를 복사해 이스케이프 줄을 12번부터 넣고 연다. 돌려주는 것: (임시 폴더, 경로, 저장소, 기대 번호).
fn store_with_escapes() -> (tempfile::TempDir, PathBuf, Store, Vec<u64>) {
    let (dir, path) = copy_of(FIXTURE_V3);
    let rows = escape_rows();
    let mut raw = Vec::new();
    let mut want = V3_LEGACY.to_vec();
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
    (dir, path, store, want)
}

// 검증: D6, V6, C2, E3
#[test]
fn 이스케이프를_푼_이름이_at_payload이면_옛_봉투_줄로_나온다() {
    let (_dir, path, store, want) = store_with_escapes();
    let last = 12 + escape_rows().len() as u64 - 1;
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(last));
    let got = all_seqs(&store);
    assert_eq!(got, want);
    for (i, (text, at, legacy)) in escape_rows().into_iter().enumerate() {
        let seq = 12 + i as u64;
        assert_eq!(
            got.contains(&seq),
            legacy,
            "{seq}번 내용={text} 시각 칸={at:?}"
        );
    }
    // 다시 열어도 같다.
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(last));
    assert_eq!(all_seqs(&store), want);
}

// 검증: D6, V6, C2, E3
#[test]
fn 이스케이프로만_드러나는_겹친_이름_줄은_나오지_않는다() {
    let (_dir, _path, store, _) = store_with_escapes();
    let got = all_seqs(&store);
    for (i, (text, _, _)) in escape_rows().into_iter().enumerate() {
        let seq = 12 + i as u64;
        if text.contains(r#""at":1,"at""#) || text.contains(r#""at":1,"at""#) {
            assert!(!got.contains(&seq), "겹친 이름 줄이 나왔다: {seq}번 {text}");
        }
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 이스케이프_이름_줄도_경계와_목록이_같은_규칙이다() {
    // 한 줄만 경계 후보 자리(12번)에 두면, 옛 봉투 줄일 때만 경계가 그 줄까지 온다.
    for (text, at, legacy) in escape_rows() {
        let (_dir, path) = copy_of(FIXTURE_V3);
        raw_insert_v3(&path, &[(12, at, text)]);
        let store = Store::open(&path).unwrap();
        let (until, want) = if legacy {
            (Some(12), vec![2, 3, 7, 10, 12])
        } else {
            (Some(10), V3_LEGACY.to_vec())
        };
        assert_eq!(
            store.legacy_envelope_until().unwrap(),
            until,
            "경계: {text} {at:?}"
        );
        assert_eq!(all_seqs(&store), want, "번호: {text} {at:?}");
        assert_eq!(
            store
                .legacy_envelope_seqs_upto(0, u64::MAX, usize::MAX)
                .unwrap(),
            want,
            "위 끝 번호: {text} {at:?}"
        );
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 버전_2_파일의_이스케이프_이름_줄도_같은_규칙이다() {
    let (_dir, path) = copy_of(FIXTURE_V2);
    assert_eq!(raw_user_version(&path), 2);
    raw_insert_v2(
        &path,
        &[
            (8, r#"{"at":1800000000008,"payload":{"ok":true}}"#),
            (9, r#"{"at":1800000000009,"payload":9}"#),
            (10, r#"{"at":1,"at":2,"payload":3}"#),
            (11, r#"{"bt":1,"payload":1}"#),
            (12, r#"{"at":-0,"payload":1}"#),
            (13, r#"{"chat":"at"}"#),
        ],
    );
    let store = Store::open(&path).unwrap();
    // 버전 2에서 올린 파일의 경계는 마지막 번호다.
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(13));
    let want = vec![2, 3, 7, 8, 9];
    assert_eq!(all_seqs(&store), want);
    assert_eq!(
        store
            .legacy_envelope_seqs_upto(0, u64::MAX, usize::MAX)
            .unwrap(),
        want
    );
    // 줄은 모두 그대로 읽힌다.
    let events = store
        .read_events(EventRange::default(), usize::MAX)
        .unwrap();
    assert_eq!(
        events.iter().map(|e| e.seq).collect::<Vec<_>>(),
        (1..=13).collect::<Vec<u64>>()
    );
    // 다시 열어도 같다.
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(13));
    assert_eq!(all_seqs(&store), want);
}

// 검증: D6, V6, C2, E3
#[test]
fn 이스케이프_이름_줄도_나눠_읽으면_전체와_같다() {
    let (_dir, _path, store, want) = store_with_escapes();
    for size in [1, 2, 3, 7] {
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

// ───────────── 2번: 위 끝을 준 옛 봉투 번호 ─────────────

/// `legacy_envelope_seqs_upto`가 따라야 하는 값: 전체 목록에서 `after`보다 크고 `upto` 이하인 것의 앞 `max`개.
fn expected_upto(all: &[u64], after: u64, upto: u64, max: usize) -> Vec<u64> {
    all.iter()
        .copied()
        .filter(|&seq| seq > after && seq <= upto)
        .take(max)
        .collect()
}

// 검증: D6, V6, C2, E3
#[test]
fn 새로_만든_파일은_위_끝을_주어도_빈_목록이다() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pi.sqlite");
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), None);
    let envelope = json!({ "at": 1_800_000_000_000_i64, "payload": { "ok": true } });
    store.append("feed_status", &envelope).unwrap();
    store
        .append("chat", &json!({ "at": 1, "payload": null }))
        .unwrap();
    for (after, upto, max) in [
        (0, 0, 0),
        (0, 1, 10),
        (0, 2, usize::MAX),
        (0, u64::MAX, usize::MAX),
        (1, u64::MAX, 10),
        (u64::MAX, u64::MAX, 10),
    ] {
        assert_eq!(
            store.legacy_envelope_seqs_upto(after, upto, max).unwrap(),
            Vec::<u64>::new(),
            "after={after} upto={upto} max={max}"
        );
    }
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store
            .legacy_envelope_seqs_upto(0, u64::MAX, usize::MAX)
            .unwrap(),
        Vec::<u64>::new()
    );
}

// 검증: D6, V6, C2, E3
#[test]
fn 위_끝_이하의_옛_봉투_줄만_나온다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    for (after, upto, max, want) in [
        (0, 10, usize::MAX, vec![2, 3, 7, 10]),
        (0, u64::MAX, usize::MAX, vec![2, 3, 7, 10]),
        (0, 11, usize::MAX, vec![2, 3, 7, 10]),
        (0, 9, usize::MAX, vec![2, 3, 7]),
        (0, 7, usize::MAX, vec![2, 3, 7]),
        (0, 6, usize::MAX, vec![2, 3]),
        (0, 3, usize::MAX, vec![2, 3]),
        (0, 2, usize::MAX, vec![2]),
        (0, 1, usize::MAX, vec![]),
        (0, 0, usize::MAX, vec![]),
        // `after`는 그 번호를 빼고, `upto`는 그 번호를 넣는다.
        (2, 7, usize::MAX, vec![3, 7]),
        (2, 3, usize::MAX, vec![3]),
        (3, 7, usize::MAX, vec![7]),
        (6, 7, usize::MAX, vec![7]),
        (7, 9, usize::MAX, vec![]),
        (7, 10, usize::MAX, vec![10]),
        (7, u64::MAX, usize::MAX, vec![10]),
        // `after`가 `upto` 이상이면 빈 목록이다.
        (3, 3, usize::MAX, vec![]),
        (7, 3, usize::MAX, vec![]),
        (10, 10, usize::MAX, vec![]),
        (10, u64::MAX, usize::MAX, vec![]),
        (u64::MAX, u64::MAX, usize::MAX, vec![]),
        // `max`개까지.
        (0, 10, 2, vec![2, 3]),
        (0, 10, 1, vec![2]),
        (0, 10, 0, vec![]),
        (1, 9, 1, vec![2]),
        (2, u64::MAX, 2, vec![3, 7]),
        (0, 6, 5, vec![2, 3]),
    ] {
        assert_eq!(
            store.legacy_envelope_seqs_upto(after, upto, max).unwrap(),
            want,
            "after={after} upto={upto} max={max}"
        );
    }
    // 다시 열어도 같다.
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.legacy_envelope_seqs_upto(2, 7, usize::MAX).unwrap(),
        vec![3, 7]
    );
}

// 검증: D6, V6, C2, E3
#[test]
fn 위_끝을_준_목록은_전체_목록을_자른_것과_같다() {
    let (_dir, _path, store, want) = store_with_escapes();
    let all = all_seqs(&store);
    assert_eq!(all, want);
    let top = *all.last().unwrap() + 2;
    for after in 0..=top {
        for upto in 0..=top {
            for max in [0, 1, 2, 3, usize::MAX] {
                assert_eq!(
                    store.legacy_envelope_seqs_upto(after, upto, max).unwrap(),
                    expected_upto(&all, after, upto, max),
                    "after={after} upto={upto} max={max}"
                );
            }
        }
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 경계_뒤의_봉투_모양_줄은_위_끝이_커도_나오지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    let at = 1_800_000_003_000_i64;
    let envelope = json!({ "at": at, "payload": { "ok": true } });
    assert_eq!(store.append("feed_status", &envelope).unwrap(), 12);
    assert_eq!(
        store
            .append("chat", &json!({ "at": 0, "payload": null }))
            .unwrap(),
        13
    );
    // 저장소를 거치지 않고 넣은 이스케이프 이름 봉투 모양 줄도 경계 뒤라 빠진다.
    drop(store);
    raw_insert_v3(&path, &[(14, None, r#"{"at":1,"payload":1}"#)]);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));

    for (after, upto) in [(0, 12), (0, 14), (0, u64::MAX), (7, 14)] {
        let want: Vec<u64> = V3_LEGACY
            .iter()
            .copied()
            .filter(|&seq| seq > after)
            .collect();
        assert_eq!(
            store
                .legacy_envelope_seqs_upto(after, upto, usize::MAX)
                .unwrap(),
            want,
            "after={after} upto={upto}"
        );
    }
    for (after, upto) in [(10, 14), (11, 14), (12, 13), (13, u64::MAX)] {
        assert_eq!(
            store
                .legacy_envelope_seqs_upto(after, upto, usize::MAX)
                .unwrap(),
            Vec::<u64>::new(),
            "after={after} upto={upto}"
        );
    }
}

// 검증: D6, V6, C2, E3
#[test]
fn 위_끝을_고정하고_나눠_읽으면_빠짐_겹침이_없다() {
    let (_dir, _path, store, want) = store_with_escapes();
    let all = all_seqs(&store);
    for upto in [0, 5, 10, 20, 30, u64::MAX] {
        let expected = expected_upto(&all, 0, upto, usize::MAX);
        for size in [1, 2, 5] {
            let mut got = Vec::new();
            let mut after = 0;
            loop {
                let page = store.legacy_envelope_seqs_upto(after, upto, size).unwrap();
                assert!(page.len() <= size);
                let Some(&last) = page.last() else { break };
                assert!(page.windows(2).all(|w| w[0] < w[1]), "번호 순서가 아니다");
                assert!(page[0] > after);
                assert!(last <= upto);
                got.extend(page);
                after = last;
            }
            assert_eq!(got, expected, "upto={upto} size={size}");
        }
    }
    assert_eq!(expected_upto(&all, 0, u64::MAX, usize::MAX), want);
}

// 검증: D6, V6, C2, E3
#[test]
fn 경계_앞에_옛_봉투가_아닌_줄이_많아도_위_끝까지만_답한다() {
    // 버전 2 파일: 8번부터 옛 봉투가 아닌 줄을 많이 두고 맨 끝에 옛 봉투 줄 하나.
    let (_dir, path) = copy_of(FIXTURE_V2);
    let mut rows: Vec<(i64, String)> = (8..2008)
        .map(|seq| (seq, format!(r#"{{"n":{seq}}}"#)))
        .collect();
    rows.push((2008, r#"{"at":1,"payload":"끝"}"#.to_string()));
    let rows: Vec<(i64, &str)> = rows.iter().map(|(s, p)| (*s, p.as_str())).collect();
    raw_insert_v2(&path, &rows);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(2008));

    assert_eq!(
        store
            .legacy_envelope_seqs_upto(7, 2007, usize::MAX)
            .unwrap(),
        Vec::<u64>::new()
    );
    assert_eq!(
        store.legacy_envelope_seqs_upto(7, 100, 10).unwrap(),
        Vec::<u64>::new()
    );
    assert_eq!(
        store.legacy_envelope_seqs_upto(0, 100, 10).unwrap(),
        vec![2, 3, 7]
    );
    assert_eq!(
        store.legacy_envelope_seqs_upto(7, 2008, 10).unwrap(),
        vec![2008]
    );
    assert_eq!(
        store
            .legacy_envelope_seqs_upto(0, u64::MAX, usize::MAX)
            .unwrap(),
        all_seqs(&store)
    );
}

// 검증: D6, V6, C2, E3
#[test]
fn 위_끝을_준_번호를_읽어도_아무것도_바뀌지_않는다() {
    let (_dir, path) = copy_of(FIXTURE_V3);
    let store = Store::open(&path).unwrap();
    let before = store
        .read_events(EventRange::default(), usize::MAX)
        .unwrap();
    for _ in 0..3 {
        assert_eq!(
            store.legacy_envelope_seqs_upto(0, 7, usize::MAX).unwrap(),
            vec![2, 3, 7]
        );
    }
    assert_eq!(
        store
            .read_events(EventRange::default(), usize::MAX)
            .unwrap(),
        before
    );
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    assert_eq!(all_seqs(&store), V3_LEGACY.to_vec());
    // 다음 번호도 그대로 이어진다.
    assert_eq!(store.append("chat", &json!({ "n": 12 })).unwrap(), 12);
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.legacy_envelope_seqs_upto(0, 7, usize::MAX).unwrap(),
        vec![2, 3, 7]
    );
}
