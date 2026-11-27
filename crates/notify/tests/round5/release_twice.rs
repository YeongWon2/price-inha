//! 조용한 시간이 끝난 뒤 `release_held`가 동시에 두 번 불려도(구독이 생길 때 · 1분마다 부르는 것이 겹침)
//! 미룬 푸시는 번호마다 한 번만 보내고, 두 부름이 돌려준 것을 합치면 미룬 것과 같다(`release_gate`).

use pi_notify::{HoldReason, NoticeKind};

use crate::common::{env_with, kst, night_quiet, notice, open_json, path_of};

// 검증: V12
#[tokio::test]
async fn 미룬_푸시를_동시에_두_번_풀어도_번호마다_한_번만_간다() {
    let env = env_with(Some(night_quiet()), 2_000).await;
    let sub = env.subscribe(1);
    env.clock.set(kst(23, 30));
    let mut held = Vec::new();
    for kind in [
        NoticeKind::DailySummary,
        NoticeKind::BackupFailure,
        NoticeKind::DailySummary,
    ] {
        let delivery = env
            .notifier
            .notify(notice(kind, kst(23, 30)))
            .await
            .unwrap();
        assert_eq!(delivery.held, Some(HoldReason::QuietHours));
        held.push(delivery.seq);
    }

    env.clock.set(kst(7, 0) + 24 * 3_600 * 1_000);
    let (first, second) = (env.notifier.clone(), env.notifier.clone());
    let (a, b) = tokio::join!(
        tokio::spawn(async move { first.release_held().await }),
        tokio::spawn(async move { second.release_held().await }),
    );
    let (a, b) = (a.unwrap().unwrap(), b.unwrap().unwrap());
    let mut sent: Vec<u64> = a.iter().chain(b.iter()).map(|d| d.seq).collect();
    sent.sort_unstable();
    assert_eq!(sent, held, "두 부름을 합치면 미룬 것이 한 번씩이다");

    let received = env.push.received_at(&path_of(&sub));
    assert_eq!(
        received.len(),
        held.len(),
        "가짜 푸시 서버가 받은 수도 같다"
    );
    let mut seqs: Vec<u64> = received
        .iter()
        .map(|got| open_json(&sub, got)["seq"].as_u64().unwrap())
        .collect();
    seqs.sort_unstable();
    assert_eq!(seqs, held, "같은 번호가 두 번 가지 않는다");

    // 세 번째는 보낼 것이 없다.
    assert!(env.notifier.release_held().await.unwrap().is_empty());
    assert_eq!(env.push.received_at(&path_of(&sub)).len(), held.len());
}
