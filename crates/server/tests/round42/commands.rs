//! 41차 5번: 명령은 그대로(결정 24 · 26 · V90) — `/status` · `/approvals` · `/reject` · `/stop` · `/help`는 39 · 40차처럼
//! 답하고, 승인은 여전히 메신저로 안 됨 · 메신저로 **나가는** 담당 답 글에 `/stop`이 들어 있어도 정지하지 않는다.

use pi_store::TeamState;
use price_inha::{
    MESSENGER_APPROVE_ON_WEB, MESSENGER_HELP, MESSENGER_NO_APPROVALS, MESSENGER_NO_SUCH_APPROVAL,
    MESSENGER_STATUS_PENDING, MESSENGER_STOPPED,
};

use crate::common::{Bench, FakeTelegram, QUIET, said};

/// 연결된 사람.
const OWNER: i64 = 87_400_001;

/// 연결된 사람이 글을 보내고, 조건에 맞는 답을 기다려 돌려준다.
async fn ask(tg: &FakeTelegram, text: &str, check: impl Fn(&str) -> bool) -> String {
    tg.push_dm(OWNER, text);
    tg.wait_sent(OWNER, text, check).await
}

// 검증: V90 V9 R14-1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 명령_다섯은_그대로_답하고_승인은_여전히_웹에서만이다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;

    let status = ask(&tg, "/status", |t| t.contains(MESSENGER_STATUS_PENDING)).await;
    assert!(status.contains("운용 중"), "{status}");
    ask(&tg, "/approvals", |t| t.contains(MESSENGER_NO_APPROVALS)).await;
    ask(&tg, "/reject zzzzzz", |t| {
        t.contains(MESSENGER_NO_SUCH_APPROVAL)
    })
    .await;
    ask(&tg, "/help", |t| t.contains(MESSENGER_HELP)).await;
    ask(&tg, "/approve 123456", |t| {
        t.contains(MESSENGER_APPROVE_ON_WEB)
    })
    .await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::Operating);
    ask(&tg, "/stop", |t| t.contains(MESSENGER_STOPPED)).await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    bench.close().await;
}

// 검증: V90 V9 V88
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당_답_글에_stop이_들어_있어도_정지하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::linked(&tg, OWNER).await;
    bench.ready().await;
    let answer = "/stop 을 보내면 팀이 멈춥니다";
    bench.ai().script_chat("strategy", answer);
    tg.push_dm(OWNER, "@전략 멈추는 법이 뭐예요?");
    tg.wait_sent(OWNER, "전략의 답", |t| {
        t.contains(&said("전략", answer))
    })
    .await;
    tokio::time::sleep(QUIET).await;
    assert_eq!(
        bench.store().team_state().unwrap(),
        TeamState::Operating,
        "나가는 글은 명령으로 읽지 않는다"
    );
    assert!(
        tg.sent()
            .await
            .iter()
            .all(|s| !s.text.contains(MESSENGER_STOPPED)),
        "정지했다는 답이 없다"
    );
    bench.close().await;
}
