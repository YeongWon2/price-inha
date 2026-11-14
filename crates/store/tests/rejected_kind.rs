#![cfg(test)]
//! 4차 3번: 거부한 이벤트 종류 이름은 잘라 담는다.
//!
//! 3차까지는 [`StoreError::InvalidEventKind`]에 받은 이름을 길이 제한 없이 그대로 담았다.
//! 4차부터 받은 이름이 [`REJECTED_KIND_KEEP_CHARS`]자보다 길면 앞 그만큼 뒤에 `…` 한 글자를 붙여 담고,
//! 그 이하이면 그대로 담는다(3차 테스트의 65자 · 200자 이름은 그대로 담긴다).

mod common;

use common::{t, temp_store};
use pi_store::{EVENT_KIND_MAX_CHARS, REJECTED_KIND_KEEP_CHARS, StoreError};
use serde_json::json;

fn rejected_kind(result: Result<u64, StoreError>) -> String {
    match result {
        Err(StoreError::InvalidEventKind { kind }) => kind,
        other => panic!("이름 규칙에 어긋나 거부되어야 한다: {other:?}"),
    }
}

/// 앞 `n`글자.
fn prefix(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// 검증: C2, E3
#[test]
fn 남기는_길이는_이름_규칙의_길이보다_길다() {
    assert_eq!(REJECTED_KIND_KEEP_CHARS, 256);
    assert_eq!(EVENT_KIND_MAX_CHARS, 64);
}

// 검증: C2, E3
#[test]
fn 아주_긴_이름은_앞부분만_잘라_담고_끝에_줄임표를_붙인다() {
    let (_dir, _path, store) = temp_store();
    for long in [
        "a".repeat(REJECTED_KIND_KEEP_CHARS + 1),
        "A".repeat(100_000),
        format!("room.{}", "x".repeat(1_000_000)),
    ] {
        let want = format!("{}…", prefix(&long, REJECTED_KIND_KEEP_CHARS));
        assert_eq!(rejected_kind(store.append(&long, &json!(1))), want);
        assert_eq!(rejected_kind(store.append_at(&long, &json!(1), t(1))), want);
    }
    // 아무것도 쌓이지 않았다.
    assert!(store.read_after(0, 10).unwrap().is_empty());
    assert_eq!(store.append("chat", &json!(1)).unwrap(), 1);
}

// 검증: C2, E3
#[test]
fn 여러_바이트_글자도_글자_단위로_자른다() {
    let (_dir, _path, store) = temp_store();
    let long = "가".repeat(REJECTED_KIND_KEEP_CHARS * 3);
    let kind = rejected_kind(store.append(&long, &json!(1)));
    assert_eq!(kind.chars().count(), REJECTED_KIND_KEEP_CHARS + 1);
    assert_eq!(kind, format!("{}…", "가".repeat(REJECTED_KIND_KEEP_CHARS)));
}

// 검증: C2, E3
#[test]
fn 남기는_길이_이하의_이름은_받은_그대로_담는다() {
    let (_dir, _path, store) = temp_store();
    let exactly = format!("A{}", "a".repeat(REJECTED_KIND_KEEP_CHARS - 1));
    assert_eq!(exactly.chars().count(), REJECTED_KIND_KEEP_CHARS);
    // 앞뒤 공백도 떼지 않은 받은 그대로다.
    let spaced = format!("  {}  ", "B".repeat(10));
    for name in [exactly.as_str(), spaced.as_str(), "Chat", "한글"] {
        assert_eq!(rejected_kind(store.append(name, &json!(1))), name);
    }
}

// 검증: C2, E3
#[test]
fn 오류_글도_잘린_이름만_담는다() {
    let (_dir, _path, store) = temp_store();
    let long = "Z".repeat(1_000_000);
    let err = store.append(&long, &json!(1)).unwrap_err();
    let message = err.to_string();
    assert!(
        message.len() < 4 * REJECTED_KIND_KEEP_CHARS + 1_000,
        "{}",
        message.len()
    );
    assert!(message.contains(&"Z".repeat(REJECTED_KIND_KEEP_CHARS)));
}
