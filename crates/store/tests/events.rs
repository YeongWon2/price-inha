#![cfg(test)]
//! 이벤트 기록 (C2 · E3)과 접속 기록 (M6).

mod common;

use std::thread;

use common::{t, temp_store};
use pi_store::{AccessRecord, NewAccess, Store};
use serde_json::json;

// 검증: C2
#[test]
fn 이벤트_번호는_1부터_빠짐없이_커진다() {
    let (_dir, _path, store) = temp_store();
    assert!(store.read_after(0, 10).unwrap().is_empty());
    for want in 1..=20_u64 {
        let seq = store.append("chat", &json!({ "n": want })).unwrap();
        assert_eq!(seq, want);
    }
    let all = store.read_after(0, 100).unwrap();
    assert_eq!(
        all.iter().map(|e| e.seq).collect::<Vec<_>>(),
        (1..=20).collect::<Vec<u64>>()
    );
}

// 검증: C2
#[test]
fn 이벤트의_종류와_내용_json이_넣은_그대로_나온다() {
    let (_dir, _path, store) = temp_store();
    let payloads = [
        (
            "chat",
            json!({ "who": "팀장", "text": "오늘은 쉰다", "tags": ["알림", 1, null] }),
        ),
        (
            "agent.position",
            json!({ "role": "시세 담당", "x": 3, "y": -7, "busy": true }),
        ),
        (
            "approval.request",
            json!({ "amount": "1250000", "nested": { "a": [[], {}] } }),
        ),
        ("notice", json!("글자 하나만")),
        ("notice", json!(null)),
        ("notice", json!([])),
    ];
    for (kind, payload) in &payloads {
        store.append(kind, payload).unwrap();
    }
    let events = store.read_after(0, 100).unwrap();
    assert_eq!(events.len(), payloads.len());
    for (i, (kind, payload)) in payloads.iter().enumerate() {
        assert_eq!(events[i].seq, i as u64 + 1);
        assert_eq!(events[i].kind, *kind);
        assert_eq!(&events[i].payload, payload);
    }
}

// 검증: C2, E3
#[test]
fn 마지막으로_받은_번호_뒤의_이벤트만_순서대로_준다() {
    let (_dir, _path, store) = temp_store();
    for n in 1..=10 {
        store.append("tick", &json!({ "n": n })).unwrap();
    }
    // 7번까지 받고 끊겼다가 다시 이어진 화면: 8, 9, 10을 놓치지 않고 받는다.
    let missed = store.read_after(7, 100).unwrap();
    assert_eq!(
        missed.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![8, 9, 10]
    );
    assert_eq!(missed[0].payload, json!({ "n": 8 }));

    // 끊긴 사이에 쌓인 것도 이어서 온다.
    store.append("tick", &json!({ "n": 11 })).unwrap();
    let last = missed.last().unwrap().seq;
    let more = store.read_after(last, 100).unwrap();
    assert_eq!(more.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![11]);

    // 다 받았으면 빈 목록이다. 아직 없는 번호를 줘도 오류가 아니라 빈 목록이다.
    assert!(store.read_after(11, 100).unwrap().is_empty());
    assert!(store.read_after(999, 100).unwrap().is_empty());
}

// 검증: C2, E3
#[test]
fn 최대_개수만큼만_주고_이어_읽으면_겹치거나_빠지지_않는다() {
    let (_dir, _path, store) = temp_store();
    for n in 1..=10 {
        store.append("tick", &json!({ "n": n })).unwrap();
    }
    assert!(store.read_after(0, 0).unwrap().is_empty());
    assert_eq!(
        store
            .read_after(0, 1)
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(
        store
            .read_after(2, 3)
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        vec![3, 4, 5]
    );
    // 끝에 걸치면 있는 만큼만 준다.
    assert_eq!(
        store
            .read_after(8, 5)
            .unwrap()
            .iter()
            .map(|e| e.seq)
            .collect::<Vec<_>>(),
        vec![9, 10]
    );

    // 3개씩 끝까지 이어 읽는다.
    let mut seen = Vec::new();
    let mut cursor = 0;
    loop {
        let page = store.read_after(cursor, 3).unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 3);
        cursor = page.last().unwrap().seq;
        seen.extend(page.into_iter().map(|e| e.seq));
    }
    assert_eq!(seen, (1..=10).collect::<Vec<u64>>());
}

fn assert_clone_send_sync<T: Clone + Send + Sync + 'static>() {}

// 검증: D6
#[test]
fn 저장소_손잡이는_복제해서_스레드_사이에_넘길_수_있다() {
    assert_clone_send_sync::<Store>();
}

// 검증: C2
#[test]
fn 여러_스레드가_함께_쌓아도_번호가_겹치거나_비지_않는다() {
    let (_dir, _path, store) = temp_store();
    let threads = 8_u64;
    let per_thread = 25_u64;
    let handles: Vec<_> = (0..threads)
        .map(|w| {
            let store = store.clone();
            thread::spawn(move || {
                (0..per_thread)
                    .map(|i| store.append("work", &json!({ "w": w, "i": i })).unwrap())
                    .collect::<Vec<u64>>()
            })
        })
        .collect();

    let mut returned = Vec::new();
    for handle in handles {
        let seqs = handle.join().unwrap();
        // 한 스레드가 받은 번호는 커지기만 한다.
        assert!(seqs.windows(2).all(|p| p[0] < p[1]));
        returned.extend(seqs);
    }
    returned.sort_unstable();
    let want: Vec<u64> = (1..=threads * per_thread).collect();
    assert_eq!(returned, want);

    let stored = store.read_after(0, 1_000).unwrap();
    assert_eq!(stored.iter().map(|e| e.seq).collect::<Vec<_>>(), want);
    // 받은 번호의 이벤트가 바로 그 내용이다.
    for e in &stored {
        assert_eq!(e.kind, "work");
        assert!(e.payload["w"].as_u64().unwrap() < threads);
    }
}

fn access(at: i64, origin: &str, action: &str) -> NewAccess {
    NewAccess {
        at: t(at),
        origin: origin.to_string(),
        action: action.to_string(),
    }
}

// 검증: M6
#[test]
fn 접속_기록은_언제_어디서_무엇을_했는지_순서대로_쌓인다() {
    let (_dir, _path, store) = temp_store();
    assert!(store.access_log().unwrap().is_empty());

    let entries = [
        (10, "127.0.0.1", "들어옴"),
        (20, "192.168.0.24", "접속 키 틀림"),
        (30, "192.168.0.24", "들어옴"),
        (40, "192.168.0.24", "다시 시작 누름"),
    ];
    for (i, (at, origin, action)) in entries.iter().enumerate() {
        let seq = store.record_access(access(*at, origin, action)).unwrap();
        assert_eq!(seq, i as u64 + 1);
    }

    let want: Vec<AccessRecord> = entries
        .iter()
        .enumerate()
        .map(|(i, (at, origin, action))| AccessRecord {
            seq: i as u64 + 1,
            at: t(*at),
            origin: origin.to_string(),
            action: action.to_string(),
        })
        .collect();
    assert_eq!(store.access_log().unwrap(), want);
}

// 검증: M6
#[test]
fn 접속_기록은_쌓이기만_하고_앞선_줄은_바뀌지_않는다() {
    let (_dir, _path, store) = temp_store();
    let mut previous: Vec<AccessRecord> = Vec::new();
    for n in 1..=15 {
        // 같은 내용이 되풀이되어도 합쳐지지 않고 한 줄씩 쌓인다.
        store
            .record_access(access(n, "192.168.0.24", "접속 키 틀림"))
            .unwrap();
        let now = store.access_log().unwrap();
        assert_eq!(now.len(), previous.len() + 1);
        assert_eq!(&now[..previous.len()], &previous[..]);
        assert_eq!(now.last().unwrap().seq, n as u64);
        assert_eq!(now.last().unwrap().at, t(n));
        previous = now;
    }
}

// 검증: M6, C2
#[test]
fn 접속_기록과_이벤트는_번호를_따로_센다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(store.append("chat", &json!({})).unwrap(), 1);
    assert_eq!(store.append("chat", &json!({})).unwrap(), 2);
    assert_eq!(
        store
            .record_access(access(1, "127.0.0.1", "들어옴"))
            .unwrap(),
        1
    );
    assert_eq!(store.append("chat", &json!({})).unwrap(), 3);
    assert_eq!(
        store.record_access(access(2, "127.0.0.1", "나감")).unwrap(),
        2
    );
    assert_eq!(store.read_after(0, 10).unwrap().len(), 3);
    assert_eq!(store.access_log().unwrap().len(), 2);
}
