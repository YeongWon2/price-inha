//! C24 · R4-4 · V55: 주문 승인 카드에 투표 수와 한도 적용 전 → 후가 실린다. 두 칸은 보여 주기용이라 지문은
//! 그대로이고, 보이는 값은 요청의 summary(저장소)에 든 것과 같다.

use pi_approval::{ApprovalNumber, Subject, fingerprint};
use pi_store::{ApprovalStatus, TradeSide};
use price_inha::{APPROVAL_PATH, KIND_OFFICE_APPROVAL};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use crate::common::{Bench, HYNIX, d, events, get, kst, monday, wait_for};

async fn start() -> Bench {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    bench.ready_for_approval().await;
    bench
}

/// 폰 · 사무실이 읽는 승인 카드(200이어야 한다).
async fn card(bench: &Bench, number: &ApprovalNumber) -> Value {
    let reply = get(
        bench.addr(),
        &APPROVAL_PATH.replace("{id}", number.as_str()),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    reply.body
}

/// 한도 키 → 화면에 보일 사유.
fn reason_of(limit: &str) -> &'static str {
    match limit {
        "risk_per_trade" => "1회 위험 한도",
        "symbol_weight" => "종목 비중 한도",
        "conviction_weight" => "확신 종목 비중 한도",
        "sector_weight" => "업종 비중 한도",
        "auto_order_limit" => "자동 주문 금액 한도",
        other => panic!("모르는 한도 키: {other}"),
    }
}

/// 저장소에 남은 그 요청의 summary.
fn stored_summary(bench: &Bench, number: &ApprovalNumber) -> Value {
    bench
        .store()
        .approvals_in_status(ApprovalStatus::Pending)
        .unwrap()
        .into_iter()
        .map(|row| row.summary)
        .find(|summary| summary["number"] == number.as_str())
        .expect("요청의 summary가 남는다")
}

// 검증: V55 C24 R4-4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_카드에_투표_수와_한도_적용_전후가_실리고_지문은_그대로다() {
    let bench = start().await;
    let trade = bench.judged_with_opinions(HYNIX, TradeSide::Buy);
    let (number, placed) = bench.awaiting(trade, "15").await;
    let body = card(&bench, &number).await;
    assert_eq!(
        body["votes"],
        json!({"bull": 1, "bear": 1, "neutral": 1, "abstain": 1}),
        "{body}"
    );
    let clamps = body["clamps"]
        .as_array()
        .unwrap_or_else(|| panic!("한도로 줄였으면 clamps가 있다: {body}"));
    assert!(!clamps.is_empty(), "{body}");
    assert_eq!(clamps[0]["before"], "15", "AI 수량에서 시작한다: {body}");
    for pair in clamps.windows(2) {
        assert_eq!(
            pair[0]["after"], pair[1]["before"],
            "앞 줄의 뒤가 다음 줄의 앞: {body}"
        );
    }
    for clamp in clamps {
        let limit = clamp["limit"].as_str().unwrap();
        assert_eq!(clamp["reason"], reason_of(limit), "{clamp}");
    }
    // "후" 수량은 승인할 주문의 수량과 같다.
    let view = bench
        .approvals()
        .get(&number)
        .unwrap()
        .expect("요청이 있다");
    let Subject::Order(order) = &view.subject else {
        panic!("주문 대상이다: {:?}", view.subject);
    };
    let last: Decimal = d(clamps.last().unwrap()["after"].as_str().unwrap());
    assert_eq!(last, Decimal::from(order.quantity), "{body}");
    // 지문은 대상만으로 정해진다(두 칸이 들어가지 않는다).
    assert_eq!(body["fingerprint"], placed.as_str());
    assert_eq!(fingerprint(&view.subject).as_str(), placed.as_str());
    // 보이는 값은 요청의 summary에 든 것과 같다.
    let stored = stored_summary(&bench, &number);
    assert_eq!(stored["votes"], body["votes"], "{stored}");
    let stored_clamps = stored["clamps"].as_array().expect("summary의 clamps");
    assert_eq!(stored_clamps.len(), clamps.len(), "{stored}");
    for (saved, shown) in stored_clamps.iter().zip(clamps) {
        for key in ["limit", "before", "after"] {
            assert_eq!(saved[key], shown[key], "{key}: {stored}");
        }
    }
    bench.close().await;
}

// 검증: V55 C24
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 사무실_승인_이벤트에도_폰_카드와_같은_투표_수와_전후가_실린다() {
    let bench = start().await;
    let trade = bench.judged_with_opinions(HYNIX, TradeSide::Buy);
    let (number, _) = bench.awaiting(trade, "15").await;
    let cards = || {
        events(&bench.store(), KIND_OFFICE_APPROVAL)
            .into_iter()
            .filter(|c| c["number"] == number.as_str())
            .collect::<Vec<_>>()
    };
    wait_for("사무실 승인 카드 이벤트", || !cards().is_empty()).await;
    let office = cards()[0].clone();
    let phone = card(&bench, &number).await;
    assert_eq!(office["status"], "pending", "{office}");
    assert!(office["votes"].is_object(), "{office}");
    assert_eq!(office["votes"], phone["votes"], "{office}");
    assert!(office["clamps"].is_array(), "{office}");
    assert_eq!(office["clamps"], phone["clamps"], "{office}");
    // 지문 앞 6자는 그대로다.
    assert_eq!(office["short_code"], phone["short_code"]);
    bench.close().await;
}

// 검증: V55 C24 E53
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 의견_기록도_줄인_수량도_없으면_두_칸이_없다() {
    let bench = start().await;
    let trade = bench.judged_plain(HYNIX);
    let (number, _) = bench.awaiting(trade, "5").await;
    let body = card(&bench, &number).await;
    assert!(body.get("votes").is_none_or(Value::is_null), "{body}");
    assert!(body.get("clamps").is_none_or(Value::is_null), "{body}");
    assert!(
        body["summary"].as_str().is_some_and(|s| !s.is_empty()),
        "{body}"
    );
    bench.close().await;
}
