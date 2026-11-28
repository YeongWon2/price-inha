#![cfg(test)]
//! 22차(결정 32): 모의계좌 스냅샷은 바뀐 때만 남긴다. 시세 관찰 칸(`ledger.last_now` · `last_quotes` ·
//! `trade_marks`)만 다르면 새 줄을 쓰지 않고, 거래일 마감에는 같아도 그날 끝값 한 줄을 남긴다.
//! (22차) 2026-11-27 백로그 작업을 합쳐 잠금.

mod common;

use common::{t, temp_store};
use pi_store::{PAPER_SNAPSHOT_MARKET_FIELDS, Store};
use serde_json::{Value, json};

/// `pi-exec` 문서와 같은 모양(모의계좌 상태 + 실행기 몫).
fn doc(cash: &str, price: &str, now: &str) -> Value {
    json!({
        "version": 4,
        "ledger": {
            "krw": {"cash": cash},
            "holdings": [{"symbol": "005930", "quantity": "10"}],
            "orders": [],
            "fills": [],
            "next_id": 3,
            "last_quotes": [{"symbol": "005930", "price": price, "received_at": now}],
            "last_now": now,
            "trade_marks": [{"symbol": "005930", "traded_at": now, "prices": [price]}]
        },
        "pi_exec": {"seq": 1, "pending": [], "positions": []}
    })
}

fn rows(path: &std::path::Path) -> i64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.query_row("SELECT COUNT(*) FROM paper_snapshots", [], |r| r.get(0))
        .unwrap()
}

// 검증: V6
#[test]
fn 시세_관찰_칸은_세_가지다() {
    assert_eq!(
        PAPER_SNAPSHOT_MARKET_FIELDS,
        ["last_now", "last_quotes", "trade_marks"]
    );
}

// 검증: V6
#[test]
fn 같은_상태를_두_번_저장하면_한_줄이고_같은_번호를_준다() {
    let (_dir, path, store) = temp_store();
    let state = doc("1000", "70000", "2026-11-20T01:00:00Z");
    assert_eq!(store.save_paper_snapshot(&state, t(1)).unwrap(), 1);
    assert_eq!(store.save_paper_snapshot(&state, t(2)).unwrap(), 1);
    assert_eq!(rows(&path), 1);
    let got = store.latest_paper_snapshot().unwrap().unwrap();
    assert_eq!(got.at, t(1), "앞 줄을 고치지 않는다");
}

// 검증: V6
#[test]
fn 돈이_바뀌면_두_줄이다() {
    let (_dir, path, store) = temp_store();
    let now = "2026-11-20T01:00:00Z";
    store
        .save_paper_snapshot(&doc("1000", "70000", now), t(1))
        .unwrap();
    let seq = store
        .save_paper_snapshot(&doc("900", "70000", now), t(2))
        .unwrap();
    assert_eq!(seq, 2);
    assert_eq!(rows(&path), 2);
}

// 검증: V6
#[test]
fn 실행기_몫이_바뀌어도_새_줄이다() {
    let (_dir, path, store) = temp_store();
    let first = doc("1000", "70000", "2026-11-20T01:00:00Z");
    store.save_paper_snapshot(&first, t(1)).unwrap();
    let mut second = first.clone();
    second["pi_exec"]["seq"] = json!(2);
    store.save_paper_snapshot(&second, t(2)).unwrap();
    assert_eq!(rows(&path), 2);
}

// 검증: V6
#[test]
fn 시세_관찰_칸만_다르면_새_줄을_쓰지_않는다() {
    let (_dir, path, store) = temp_store();
    store
        .save_paper_snapshot(&doc("1000", "70000", "2026-11-20T01:00:00Z"), t(1))
        .unwrap();
    for minute in 1..30 {
        let now = format!("2026-11-20T01:{minute:02}:00Z");
        let price = format!("{}", 70_000 + minute * 10);
        let seq = store
            .save_paper_snapshot(&doc("1000", &price, &now), t(1 + minute))
            .unwrap();
        assert_eq!(seq, 1);
    }
    assert_eq!(rows(&path), 1);
}

// 검증: V6
#[test]
fn 마감에는_같아도_그날_끝값_한_줄을_남긴다() {
    let (_dir, path, store) = temp_store();
    let state = doc("1000", "70000", "2026-11-20T01:00:00Z");
    store.save_paper_snapshot(&state, t(1)).unwrap();
    store.save_paper_snapshot(&state, t(2)).unwrap();
    assert_eq!(rows(&path), 1);
    let seq = store.save_paper_day_end(t(9)).unwrap();
    assert_eq!(seq, Some(2));
    assert_eq!(rows(&path), 2);
    let got = store.latest_paper_snapshot().unwrap().unwrap();
    assert_eq!(got.state, state);
    assert_eq!(got.at, t(9));
}

// 검증: V6
#[test]
fn 마감_끝값은_건너뛴_가장_나중_상태다() {
    let (_dir, path, store) = temp_store();
    store
        .save_paper_snapshot(&doc("1000", "70000", "2026-11-20T01:00:00Z"), t(1))
        .unwrap();
    let last = doc("1000", "71500", "2026-11-20T06:30:00Z");
    store
        .save_paper_snapshot(&doc("1000", "70500", "2026-11-20T03:00:00Z"), t(2))
        .unwrap();
    store.save_paper_snapshot(&last, t(3)).unwrap();
    assert_eq!(rows(&path), 1);
    store.save_paper_day_end(t(4)).unwrap();
    assert_eq!(rows(&path), 2);
    assert_eq!(store.latest_paper_snapshot().unwrap().unwrap().state, last);
    // 끝값을 남긴 뒤 같은 상태는 다시 쓰지 않는다.
    store.save_paper_snapshot(&last, t(5)).unwrap();
    assert_eq!(rows(&path), 2);
}

// 검증: V6
#[test]
fn 상태가_없으면_마감_끝값도_없다() {
    let (_dir, path, store) = temp_store();
    assert_eq!(store.save_paper_day_end(t(1)).unwrap(), None);
    assert_eq!(rows(&path), 0);
}

// 검증: V6 D6
#[test]
fn 다시_켠_뒤_되살린_상태가_마지막_상태와_같고_같은_상태는_다시_쓰지_않는다() {
    let (_dir, path, store) = temp_store();
    let now = "2026-11-20T01:00:00Z";
    store
        .save_paper_snapshot(&doc("1000", "70000", now), t(1))
        .unwrap();
    let last = doc("800", "70000", now);
    store.save_paper_snapshot(&last, t(2)).unwrap();
    std::mem::forget(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.latest_paper_snapshot().unwrap().unwrap().state, last);
    assert_eq!(store.save_paper_snapshot(&last, t(3)).unwrap(), 2);
    assert_eq!(rows(&path), 2);
}

// 검증: V6
#[test]
fn 스냅샷_표는_여전히_쌓기만_하고_지울_수_없다() {
    let (_dir, path, store) = temp_store();
    store
        .save_paper_snapshot(&doc("1000", "70000", "2026-11-20T01:00:00Z"), t(1))
        .unwrap();
    store.save_paper_day_end(t(2)).unwrap();
    let conn = rusqlite::Connection::open(&path).unwrap();
    assert!(conn.execute("DELETE FROM paper_snapshots", []).is_err());
    assert!(
        conn.execute("UPDATE paper_snapshots SET state = '{}'", [])
            .is_err()
    );
    assert_eq!(rows(&path), 2);
}
