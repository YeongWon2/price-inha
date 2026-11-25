//! 40차 3번: 메신저 `/stop` — 허용된 사람은 같은 길로 전체 정지하고 답 · 이미 정지면 그 답 · 허용 안 된 사람은 그대로 ·
//! 도움말에 `/stop` · 메신저로 다시 시작은 안 됨(결정 25 · 24 · V90 · V88).

use pi_store::{TeamCause, TeamState};
use price_inha::{MESSENGER_ALREADY_STOPPED, MESSENGER_HELP, MESSENGER_STOPPED, TEAM_STOP_PATH};

use crate::common::{Bench, FakeTelegram, QUIET, is_pairing_reply, post_empty};

/// 연결된 사람.
const OWNER: i64 = 86_000_001;

/// 연결된 사람이 명령을 보내고, 조건에 맞는 답을 기다려 돌려준다.
async fn ask(tg: &FakeTelegram, text: &str, check: impl Fn(&str) -> bool) -> String {
    tg.push_dm(OWNER, text);
    tg.wait_sent(OWNER, text, check).await
}

// 검증: V90 V9 R14-1 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 허용된_사람의_stop은_전체_정지하고_답한다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, OWNER).await;
    bench.ready().await;

    let reply = ask(&tg, "/stop", |t| t.contains(MESSENGER_STOPPED)).await;
    assert!(reply.contains(MESSENGER_STOPPED), "{reply}");
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    let record = bench
        .store()
        .team_history()
        .unwrap()
        .last()
        .cloned()
        .unwrap();
    assert_eq!(record.cause, TeamCause::OwnerStop, "{record:?}");
    assert!(record.owner_approved, "{record:?}");
    assert_eq!(record.from, TeamState::Operating, "{record:?}");
    bench.close().await;
}

// 검증: V90 V9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 이미_정지면_그렇다고_답하고_기록을_더하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, OWNER).await;
    let stopped = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_eq!(stopped.status, 200, "{}", stopped.text);
    let count = bench.store().team_history().unwrap().len();

    let reply = ask(&tg, "/stop", |t| t.contains(MESSENGER_ALREADY_STOPPED)).await;
    assert!(reply.contains(MESSENGER_ALREADY_STOPPED), "{reply}");
    assert!(!reply.contains(MESSENGER_STOPPED), "{reply}");
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    assert_eq!(bench.store().team_history().unwrap().len(), count);
    bench.close().await;
}

// 검증: V88 V90 V9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 허용되지_않은_사람의_stop은_팀을_멈추지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, OWNER).await;
    bench.ready().await;

    let stranger: i64 = 86_000_099;
    tg.push_dm(stranger, "/stop");
    tg.wait_sent(stranger, "모르는 사람에게는 코드", is_pairing_reply)
        .await;
    tg.push_group(OWNER, -1_009_000_041, "/stop");
    tokio::time::sleep(QUIET).await;

    assert_eq!(bench.store().team_state().unwrap(), TeamState::Operating);
    assert!(
        tg.sent()
            .await
            .iter()
            .all(|s| !s.text.contains(MESSENGER_STOPPED)
                && !s.text.contains(MESSENGER_ALREADY_STOPPED)),
        "정지했다는 답이 없다"
    );
    bench.close().await;
}

// 검증: V90
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 도움말에_stop이_다시_든다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, OWNER).await;

    assert!(MESSENGER_HELP.contains("/stop"), "{MESSENGER_HELP}");
    let help = ask(&tg, "/help", |t| t.contains(MESSENGER_HELP)).await;
    for command in ["/status", "/approvals", "/reject", "/stop", "/help"] {
        assert!(help.contains(command), "도움말에 {command}: {help}");
    }
    bench.close().await;
}

// 검증: V90 R14-1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 메신저로는_다시_시작하지_않는다() {
    // 다시 시작은 웹(계좌 화면)에서만 한다(결정 20 · 24). 정지한 뒤 다시 시작을 뜻하는 글을 보내도 그대로다.
    let tg = FakeTelegram::start().await;
    let bench = Bench::with_token(&tg).await;
    bench.pair(&tg, OWNER).await;
    let _ = ask(&tg, "/stop", |t| t.contains(MESSENGER_STOPPED)).await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);

    for text in ["/restart", "/start", "/resume"] {
        tg.push_dm(OWNER, text);
    }
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    bench.close().await;
}
