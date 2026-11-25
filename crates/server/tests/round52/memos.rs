//! `GET /api/memos`(C25): `?days=`는 배열, `?before=&max=`는 히스토리와 같은 쪽 넘기기, 틀린 값 400, 원격 401.

use serde_json::Value;

use crate::common::{Bench, MEMO_SYMBOL, MEMOS, get};

/// 쪽 응답의 매매 번호들(보인 순서).
fn trades_of(body: &Value) -> Vec<i64> {
    body["memos"]
        .as_array()
        .expect("memos 배열")
        .iter()
        .map(|memo| memo["trade"].as_i64().expect("매매 번호"))
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 최근_결정_메모는_최근_것부터이고_히스토리처럼_쪽을_넘긴다() {
    // 검증: R7-4 C25
    let bench = Bench::start().await;
    let first = bench.judged_buy(MEMO_SYMBOL).await;
    let second = bench.judged_buy(MEMO_SYMBOL).await;
    let third = bench.judged_buy(MEMO_SYMBOL).await;

    // `?days=`: 배열, 최근 것부터, 그때 까닭이 함께 온다.
    let reply = bench.get(&format!("{MEMOS}?days=7")).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let list = reply.body.as_array().expect("배열");
    let shown: Vec<i64> = list.iter().filter_map(|m| m["trade"].as_i64()).collect();
    let mine: Vec<i64> = shown
        .iter()
        .copied()
        .filter(|t| [first.0, second.0, third.0].contains(t))
        .collect();
    assert_eq!(mine, vec![third.0, second.0, first.0], "{}", reply.text);
    let memo = list.iter().find(|m| m["trade"] == third.0).unwrap();
    assert_eq!(memo["reason"], "시험 가설 실적 개선", "{memo}");

    // 인자 없이는 `?days=7`과 같다.
    let plain = bench.get(MEMOS).await;
    assert_eq!(plain.status, 200, "{}", plain.text);
    assert_eq!(plain.body, reply.body);

    // `?max=`: 히스토리와 같은 쪽(`next`는 마지막 줄의 번호, 더 없으면 null).
    let page = bench.get(&format!("{MEMOS}?max=2")).await;
    assert_eq!(page.status, 200, "{}", page.text);
    assert_eq!(
        trades_of(&page.body),
        vec![third.0, second.0],
        "{}",
        page.text
    );
    let next = page.body["next"].as_i64().expect("다음 쪽이 있다");
    let rest = bench.get(&format!("{MEMOS}?max=2&before={next}")).await;
    assert_eq!(rest.status, 200, "{}", rest.text);
    assert_eq!(trades_of(&rest.body), vec![first.0], "{}", rest.text);
    assert!(rest.body["next"].is_null(), "{}", rest.text);
    bench.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 틀린_값은_400이고_원격은_세션_없이_401이다() {
    // 검증: C25
    let bench = Bench::start().await;
    for bad in [
        "days=0",
        "days=-1",
        "days=abc",
        "days=400",
        "max=0",
        "max=abc",
        "before=0",
        "before=abc",
    ] {
        let reply = bench.get(&format!("{MEMOS}?{bad}")).await;
        assert_eq!(reply.status, 400, "{bad}: {}", reply.text);
    }
    let empty = bench.get(&format!("{MEMOS}?days=1")).await;
    assert_eq!(empty.status, 200);
    assert!(empty.body.is_array(), "{}", empty.text);

    bench.go_remote();
    let reply = get(bench.addr(), MEMOS, None).await;
    assert_eq!(reply.status, 401, "{}", reply.text);
    bench.close().await;
}
