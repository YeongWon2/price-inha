#![cfg(test)]
//! 9차 4번 · 5번: 모의계좌 상태 값(V6)과 접속 키 해시 · 소금 · 세대(M6).

mod common;

use common::{t, temp_store};
use pi_store::{ACCESS_KEY_SALT_MIN_BYTES, AccessKeyHash, Store, StoreError};
use serde_json::{Value, json};

/// `pi-paper` 상태 값과 같은 모양의 값(돈과 수량은 십진수 글자, 시각은 RFC 3339 글자).
fn paper_state(cash: &str) -> Value {
    json!({
        "version": 4,
        "ledger": {
            "next_id": 18_446_744_073_709_551_615_u64,
            "cash": cash,
            "holdings": [
                { "symbol": "005930", "quantity": "10", "average_purchase_price": "70123.4567" }
            ],
            "orders": [
                {
                    "status": "Filled", "quantity": "10", "held_cash": "0",
                    "filled_quantity": "10", "filled_amount": "701234.567",
                    "commission": "105.18", "tax": "0", "currency": "Krw"
                }
            ],
            "last_quotes": [
                { "symbol": "005930", "price": "70100", "received_at": "2027-01-04T00:00:01.123456789Z" }
            ],
            "trade_marks": [],
            "메모": "한글 \u{1F600} \"따옴표\" \\ 뒤빗금",
            "음수": -9_223_372_036_854_775_808_i64,
            "없음": null,
            "참": true
        }
    })
}

fn hash(byte: u8, salt_len: usize) -> AccessKeyHash {
    AccessKeyHash {
        sha256: [byte; 32],
        salt: vec![byte ^ 0x5a; salt_len],
    }
}

// 검증: V6
#[test]
fn 새_저장소에는_모의계좌_상태_값이_없다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(store.latest_paper_snapshot().unwrap(), None);
}

// 검증: V6
#[test]
fn 모의계좌_상태_값은_넣은_그대로_나온다() {
    let (_dir, _path, store) = temp_store();
    let seq = store
        .save_paper_snapshot(&paper_state("1000000.00"), t(5))
        .unwrap();
    assert_eq!(seq, 1);
    let got = store.latest_paper_snapshot().unwrap().unwrap();
    assert_eq!(got.seq, 1);
    assert_eq!(got.state, paper_state("1000000.00"));
    assert_eq!(got.at, t(5));
}

// 검증: V6
#[test]
fn 가장_나중에_저장한_상태_값이_나오고_번호는_빠짐없이_커진다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(
        store.save_paper_snapshot(&paper_state("1"), t(10)).unwrap(),
        1
    );
    assert_eq!(
        store.save_paper_snapshot(&paper_state("2"), t(20)).unwrap(),
        2
    );
    // 시각이 앞서도 번호가 큰 것이 가장 나중이다.
    assert_eq!(
        store.save_paper_snapshot(&paper_state("3"), t(15)).unwrap(),
        3
    );
    let got = store.latest_paper_snapshot().unwrap().unwrap();
    assert_eq!(got.seq, 3);
    assert_eq!(got.state, paper_state("3"));
    assert_eq!(got.at, t(15));
}

// 검증: V6, D6
#[test]
fn 껐다_켜도_모의계좌_상태_값이_이어진다() {
    let (_dir, path, store) = temp_store();
    store
        .save_paper_snapshot(&paper_state("500.5"), t(1))
        .unwrap();
    // 손잡이를 닫지 않고 잃어도(갑자기 꺼짐) 그대로다.
    std::mem::forget(store);
    let store = Store::open(&path).unwrap();
    let got = store.latest_paper_snapshot().unwrap().unwrap();
    assert_eq!(got.state, paper_state("500.5"));
    assert_eq!(
        store
            .save_paper_snapshot(&paper_state("600"), t(2))
            .unwrap(),
        2
    );
}

// 검증: M6
#[test]
fn 접속_키를_둔_적이_없으면_없음이다() {
    let (_dir, _path, store) = temp_store();
    assert_eq!(store.access_key().unwrap(), None);
}

// 검증: M6
#[test]
fn 접속_키_해시는_세대_1부터_바꿀_때마다_1씩_커진다() {
    let (_dir, _path, store) = temp_store();
    let first = store.set_access_key(hash(1, 16), t(1)).unwrap();
    assert_eq!(first.generation, 1);
    assert_eq!(first.sha256, [1; 32]);
    assert_eq!(first.salt, vec![1 ^ 0x5a; 16]);
    assert_eq!(first.set_at, t(1));
    assert_eq!(store.access_key().unwrap(), Some(first));

    let second = store.set_access_key(hash(2, 32), t(2)).unwrap();
    assert_eq!(second.generation, 2);
    assert_eq!(second.sha256, [2; 32]);
    assert_eq!(second.salt, vec![2 ^ 0x5a; 32]);
    // 지금 키는 새 세대 하나다.
    assert_eq!(store.access_key().unwrap(), Some(second));
}

// 검증: M6
#[test]
fn 짧은_소금은_받지_않는다() {
    assert_eq!(ACCESS_KEY_SALT_MIN_BYTES, 16);
    let (_dir, _path, store) = temp_store();
    for len in [0, 1, ACCESS_KEY_SALT_MIN_BYTES - 1] {
        let err = store.set_access_key(hash(3, len), t(1)).unwrap_err();
        assert!(
            matches!(err, StoreError::AccessKeySaltTooShort { len: l } if l == len),
            "{len}"
        );
    }
    assert_eq!(store.access_key().unwrap(), None);
    // 거부는 세대를 쓰지 않는다.
    let first = store
        .set_access_key(hash(3, ACCESS_KEY_SALT_MIN_BYTES), t(2))
        .unwrap();
    assert_eq!(first.generation, 1);
}

// 검증: M6, D6
#[test]
fn 다시_열어도_접속_키_해시와_세대가_이어진다() {
    let (_dir, path, store) = temp_store();
    store.set_access_key(hash(4, 16), t(1)).unwrap();
    let second = store.set_access_key(hash(5, 20), t(2)).unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(store.access_key().unwrap(), Some(second));
    let third = store.set_access_key(hash(6, 16), t(3)).unwrap();
    assert_eq!(third.generation, 3);
}
