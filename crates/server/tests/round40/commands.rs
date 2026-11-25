//! 39차 5번: 허용된 사람의 명령 — status · approvals · reject · stop · help · 승인 글은 웹에서만 · 허용 안 된 사람의
//! 명령은 실행 안 됨(결정 24 · V90 · V88).

use pi_approval::{ApprovalNumber, Fingerprint};
use pi_store::TeamState;
use price_inha::{
    APPROVAL_ANSWER_PATH, KIND_OFFICE_APPROVAL, MESSENGER_APPROVE_ON_WEB,
    MESSENGER_APPROVE_ON_WEB_LINK, MESSENGER_HELP, MESSENGER_NO_SUCH_APPROVAL, MESSENGER_REJECTED,
    MESSENGER_STATUS_PENDING, MESSENGER_STOP_LATER, MESSENGER_UNKNOWN, OFFICE_PATH,
    OFFICE_PENDING_APPROVALS,
};
use serde_json::{Value, json};

use crate::common::{
    Bench, FakeTelegram, QUIET, events, get, is_pairing_reply, post, short, wait_for,
};

/// 연결된 사람.
const OWNER: i64 = 83_000_001;

/// 토큰 · 연결 · 운용 중까지 맞춘 서버.
async fn paired_bench(tg: &FakeTelegram) -> Bench {
    let bench = Bench::with_token(tg).await;
    bench.pair(tg, OWNER).await;
    bench.ready().await;
    bench
}

/// 연결된 사람이 명령을 보내고, 조건에 맞는 답을 기다려 돌려준다.
async fn ask(tg: &FakeTelegram, text: &str, check: impl Fn(&str) -> bool) -> String {
    tg.push_dm(OWNER, text);
    tg.wait_sent(OWNER, text, check).await
}

/// 그 승인 번호의 사무실 승인 이벤트들.
fn cards(bench: &Bench, number: &ApprovalNumber) -> Vec<Value> {
    events(&bench.store(), KIND_OFFICE_APPROVAL)
        .into_iter()
        .filter(|card| card["number"] == json!(number.as_str()))
        .collect()
}

/// 웹(지금 승인 길)으로 승인해 본 결과.
async fn approve_on_web(
    bench: &Bench,
    number: &ApprovalNumber,
    fingerprint: &Fingerprint,
) -> Value {
    let body = json!({
        "number": number.as_str(),
        "fingerprint": fingerprint.as_str(),
        "answer": "approve",
    });
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body
}

/// 사무실 스냅샷의 기다리는 승인 번호들.
async fn waiting_numbers(bench: &Bench) -> Vec<String> {
    let reply = get(bench.addr(), OFFICE_PATH).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    reply.body[OFFICE_PENDING_APPROVALS]
        .as_array()
        .unwrap_or_else(|| panic!("{OFFICE_PENDING_APPROVALS} 칸은 배열이다: {}", reply.text))
        .iter()
        .filter_map(|row| row["number"].as_str().map(str::to_string))
        .collect()
}

/// 그 승인이 여전히 기다림(대기 목록에 있다)이고, 그 뒤 웹(지금 승인 길)에서 거절하면 거절된다.
/// 웹 승인은 P12 승인 재점검(멈춘 시험 시계에서는 종목 상태가 "모름")에 걸리므로 거절 길로 본다(39차 시험 결함 고침 2).
async fn still_waiting_then_reject_on_web(
    bench: &Bench,
    number: &ApprovalNumber,
    fingerprint: &Fingerprint,
) {
    let waiting = waiting_numbers(bench).await;
    assert!(
        waiting.iter().any(|n| n == number.as_str()),
        "승인은 여전히 기다림: {waiting:?}"
    );
    let body = json!({
        "number": number.as_str(),
        "fingerprint": fingerprint.as_str(),
        "answer": "reject",
    });
    let reply = post(bench.addr(), APPROVAL_ANSWER_PATH, &body).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(
        reply.body["result"],
        json!("rejected"),
        "웹 거절 길은 그대로: {}",
        reply.text
    );
    let after = waiting_numbers(bench).await;
    assert!(
        after.iter().all(|n| n != number.as_str()),
        "거절한 승인은 대기 목록에서 빠진다: {after:?}"
    );
}

// 검증: V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_명령은_팀_상태와_보유와_기다리는_승인_수를_한_덩이로_답한다() {
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;
    bench.awaiting().await;

    let pending = format!("{MESSENGER_STATUS_PENDING} 1");
    let reply = ask(&tg, "/status", |t| t.contains(MESSENGER_STATUS_PENDING)).await;
    assert!(reply.contains(&pending), "기다리는 승인 수: {reply}");
    assert!(reply.contains("보유"), "보유 수: {reply}");
    assert!(reply.contains("운용 중"), "팀 상태: {reply}");
    bench.close().await;
}

// 검증: V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn approvals_명령은_짧은_번호와_웹_안내를_답한다() {
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;
    let (number, fingerprint) = bench.awaiting().await;

    let reply = ask(&tg, "/approvals", |t| {
        t.contains(MESSENGER_APPROVE_ON_WEB_LINK)
    })
    .await;
    assert!(reply.contains(&short(&number)), "짧은 번호: {reply}");
    assert!(reply.contains(crate::common::HYNIX), "한 줄 요약: {reply}");
    assert!(
        !reply.contains(number.as_str()),
        "번호 전체는 싣지 않는다: {reply}"
    );
    assert!(!reply.contains(fingerprint.as_str()), "지문은 싣지 않는다");
    bench.close().await;
}

// 검증: V90 C7
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reject_명령은_그_승인을_거절하고_사무실_승인_이벤트가_answered다() {
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;
    let (number, fingerprint) = bench.awaiting().await;

    let command = format!("/reject {}", short(&number));
    let reply = ask(&tg, &command, |t| t.contains(MESSENGER_REJECTED)).await;
    assert!(reply.contains(MESSENGER_REJECTED), "{reply}");
    wait_for("거절된 사무실 승인 이벤트", || {
        cards(&bench, &number)
            .iter()
            .any(|card| card["state"] == json!("answered"))
    })
    .await;
    let answered = cards(&bench, &number)
        .into_iter()
        .find(|card| card["state"] == json!("answered"))
        .unwrap();
    assert_eq!(answered["status"], json!("rejected"), "{answered}");

    let late = approve_on_web(&bench, &number, &fingerprint).await;
    assert_eq!(late["result"], json!("rejected"), "{late}");
    assert_eq!(
        late["reason"],
        json!("already_resolved"),
        "첫 결과(거절)가 남는다: {late}"
    );
    bench.close().await;
}

// 검증: V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 없는_번호의_거절은_까닭을_답하고_아무것도_바꾸지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;
    let (number, fingerprint) = bench.awaiting().await;

    let reply = ask(&tg, "/reject zzzzzz", |t| {
        t.contains(MESSENGER_NO_SUCH_APPROVAL)
    })
    .await;
    assert!(reply.contains(MESSENGER_NO_SUCH_APPROVAL), "{reply}");
    tokio::time::sleep(QUIET).await;
    assert!(
        cards(&bench, &number)
            .iter()
            .all(|card| card["state"] != json!("answered")),
        "기다리는 승인은 그대로"
    );
    still_waiting_then_reject_on_web(&bench, &number, &fingerprint).await;
    bench.close().await;
}

// 검증: V90 V9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_명령은_아직_정지하지_않고_그렇다고_답한다() {
    // 사람이 내리는 전체 정지는 저장소의 팀 상태 전이에 아직 길이 없다(최대 손실 사유만 받는다).
    // 그 길이 생기는 차수 전까지 메신저의 /stop은 팀 상태를 바꾸지 않고 그렇다고 답한다(39차 5번 고침).
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::Operating);

    let reply = ask(&tg, "/stop", |t| t.contains(MESSENGER_STOP_LATER)).await;
    assert!(reply.contains(MESSENGER_STOP_LATER), "{reply}");
    assert_eq!(bench.store().team_state().unwrap(), TeamState::Operating);
    bench.close().await;
}

// 검증: V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn help와_모르는_글에는_도움말을_답한다() {
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;

    let help = ask(&tg, "/help", |t| t.contains(MESSENGER_HELP)).await;
    for command in ["/status", "/approvals", "/reject"] {
        assert!(help.contains(command), "도움말에 {command}: {help}");
    }
    let unknown = ask(&tg, "오늘 날씨 어때", |t| {
        t.contains(MESSENGER_UNKNOWN)
    })
    .await;
    assert!(unknown.contains(MESSENGER_UNKNOWN), "{unknown}");
    bench.close().await;
}

// 검증: V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 승인하려는_글에는_웹에서만이라고_답하고_승인은_그대로다() {
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;
    let (number, fingerprint) = bench.awaiting().await;

    let command = format!("/approve {}", short(&number));
    let reply = ask(&tg, &command, |t| t.contains(MESSENGER_APPROVE_ON_WEB)).await;
    assert!(reply.contains(MESSENGER_APPROVE_ON_WEB), "{reply}");
    tokio::time::sleep(QUIET).await;
    assert!(
        cards(&bench, &number)
            .iter()
            .all(|card| card["state"] != json!("answered")),
        "메신저로는 승인하지 않는다"
    );
    still_waiting_then_reject_on_web(&bench, &number, &fingerprint).await;
    bench.close().await;
}

// 검증: V88 V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 허용되지_않은_사람의_명령은_실행하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = paired_bench(&tg).await;
    let (number, _) = bench.awaiting().await;

    let stranger: i64 = 83_000_099;
    tg.push_dm(stranger, "/stop");
    tg.wait_sent(stranger, "모르는 사람에게는 코드", is_pairing_reply)
        .await;
    tg.push_group(
        OWNER,
        -1_009_000_888,
        &format!("/reject {}", short(&number)),
    );
    tokio::time::sleep(QUIET).await;

    assert_eq!(
        bench.store().team_state().unwrap(),
        TeamState::Operating,
        "멈추지 않는다"
    );
    assert!(
        cards(&bench, &number)
            .iter()
            .all(|card| card["state"] != json!("answered")),
        "그룹 글의 거절도 실행하지 않는다"
    );
    assert!(
        tg.sent()
            .await
            .iter()
            .all(|s| !s.text.contains(MESSENGER_STOP_LATER) && !s.text.contains(MESSENGER_REJECTED)),
        "실행했다는 답이 없다"
    );
    bench.close().await;
}
