//! 발판: 지금 구현에서 서버가 뜨고, 팀 방에서 멘션한 담당이 곁으로 온다(`office.summon` called · mention).

use pi_agent::Role;
use price_inha::{DemoTurn, OFFICE_CHAT_PATH};

use crate::common::{Bench, has_phase, summons, team_text, wait_for};

// 검증: V59 C20
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 발판_멘션한_담당은_곁으로_온다() {
    let bench = Bench::start().await;
    bench.script(DemoTurn::Chat(Role::Review), "복기입니다.");
    let reply = bench
        .post(OFFICE_CHAT_PATH, &team_text("@[복기] 잠깐요", "f-1"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    wait_for("멘션된 담당이 오는 중이 된다", || {
        has_phase(&bench.store(), "review", "called")
    })
    .await;
    let called = summons(&bench.store(), "review")
        .into_iter()
        .find(|e| e["phase"] == "called")
        .unwrap();
    assert_eq!(called["reason"], "mention", "{called}");
    bench.close().await;
}
