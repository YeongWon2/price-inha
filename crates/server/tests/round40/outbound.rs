//! 39차 6번: 나가는 알림 — 폰 푸시로 가는 알림을 허용된 대화에도(가린 글 + 승인 요청엔 거절 안내) · 허용된 사람이
//! 없으면 보내지 않음 · 보내기 실패는 승인 길을 막지 않음(결정 24 · V89 · N8 · E38).

use pi_approval::ApprovalNumber;
use pi_secrets::ProviderKeys;
use price_inha::{
    APPROVAL_ANSWER_PATH, KIND_OFFICE_APPROVAL, MESSENGER_REJECT_HINT, MESSENGER_WEB_ONLY,
    OFFICE_PATH, OFFICE_PENDING_APPROVALS,
};
use serde_json::json;

use crate::common::{
    Bench, FakeTelegram, HYNIX, QUIET, TOKEN, events, fresh_home, get, post, register_provider,
    short, wait_for,
};

/// 연결된 사람.
const OWNER: i64 = 84_000_001;

/// 승인 요청 알림 본문에 들어가는 말(주문 금액 한도 까닭). 비밀값으로 등록해 가려지는지 본다.
const BAIT: &str = "자동 주문 금액 한도";

/// 시험용 공급자 키(가리기 대상을 하나 더 둔다).
const BAIT_KEYS: ProviderKeys = ProviderKeys::new("siheom", &["api_key"]);

/// 그 승인의 사무실 카드가 났다(승인 요청 훅이 돌았다).
async fn wait_card(bench: &Bench, number: &ApprovalNumber) {
    let store = bench.store();
    wait_for("사무실 승인 카드", || {
        events(&store, KIND_OFFICE_APPROVAL)
            .iter()
            .any(|card| card["number"] == json!(number.as_str()))
    })
    .await;
}

// 검증: V89 V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인_요청이_생기면_허용된_대화로_요약과_거절_안내가_간다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, OWNER).await;
    bench.ready().await;
    let (number, fingerprint) = bench.awaiting().await;

    let hint = format!(
        "{MESSENGER_REJECT_HINT}{}{MESSENGER_WEB_ONLY}",
        short(&number)
    );
    let alert = tg
        .wait_sent(OWNER, "승인 요청 알림", |t| {
            t.contains(MESSENGER_REJECT_HINT)
        })
        .await;
    assert!(alert.contains(HYNIX), "알림의 요약 그대로: {alert}");
    assert!(alert.contains(&hint), "거절 안내: {alert}");
    assert!(
        !alert.contains(number.as_str()),
        "번호 전체는 싣지 않는다: {alert}"
    );
    assert!(!alert.contains(fingerprint.as_str()), "지문은 싣지 않는다");
    assert!(!alert.contains(TOKEN));
    bench.close().await;
}

// 검증: V89 N8 E38
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 비밀값이_섞인_알림은_가려져_나간다() {
    let tg = FakeTelegram::start().await;
    let home = fresh_home().await;
    register_provider(home.path(), BAIT_KEYS, "api_key", BAIT).await;
    let bench = Bench::start_in(home, &tg).await;
    let reply = bench.register(TOKEN).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    bench.wait_state("polling").await;
    bench.pair(&tg, OWNER).await;
    bench.ready().await;
    bench.awaiting().await;

    let alert = tg
        .wait_sent(OWNER, "승인 요청 알림", |t| {
            t.contains(MESSENGER_REJECT_HINT)
        })
        .await;
    assert!(alert.contains(HYNIX), "알림은 간다: {alert}");
    assert!(!alert.contains(BAIT), "비밀값은 가려진다: {alert}");
    for sent in tg.sent().await {
        assert!(!sent.text.contains(BAIT), "어느 글에도 없다: {}", sent.text);
    }
    bench.close().await;
}

// 검증: V89
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 허용된_사람이_없으면_알림을_보내지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.ready().await;
    let (number, _) = bench.awaiting().await;
    wait_card(&bench, &number).await;
    tokio::time::sleep(QUIET).await;
    assert!(tg.sent().await.is_empty(), "보낼 대화가 없다");
    bench.close().await;
}

// 검증: V89 P20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 보내기가_실패해도_승인_길은_그대로이고_정한_만큼만_다시_보낸다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, OWNER).await;
    bench.ready().await;
    let before = tg.count("sendMessage").await;
    tg.fail_send(true);

    let (number, fingerprint) = bench.awaiting().await;
    wait_card(&bench, &number).await;
    tg.wait_count("sendMessage", before + 3).await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        tg.count("sendMessage").await,
        before + 3,
        "물러서며 3번까지 보내 보고 버린다"
    );

    // 승인 길이 도는지는 웹 거절로 본다: 웹 승인은 P12 승인 재점검(멈춘 시험 시계에서는 종목 상태가 "모름")에
    // 걸리므로(39차 시험 결함 고침 2). 승인은 여전히 기다림(대기 목록에 있다)이고, 웹에서 거절하면 거절된다.
    let office = get(bench.addr(), OFFICE_PATH).await;
    assert_eq!(office.status, 200, "{}", office.text);
    let waiting = office.body[OFFICE_PENDING_APPROVALS]
        .as_array()
        .unwrap_or_else(|| panic!("{OFFICE_PENDING_APPROVALS} 칸은 배열이다: {}", office.text));
    assert!(
        waiting
            .iter()
            .any(|row| row["number"] == json!(number.as_str())),
        "승인은 여전히 기다림: {}",
        office.text
    );
    let body = json!({
        "number": number.as_str(),
        "fingerprint": fingerprint.as_str(),
        "answer": "reject",
    });
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["result"], json!("rejected"), "{}", reply.text);
    bench.close().await;
}
