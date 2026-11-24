//! 16차 3번: 하루 AI 호출 상한(설정 `TeamWiring::ai_daily_limit`, 기본 비움 = 상한 없음). 켜져 있으면 한국 시각
//! 하루 안에 팀 논의로 AI를 부른 횟수를 세고, 상한에 닿으면 그날 남은 논의를 쉬고 한 번 알린다. 다음 날 다시 열리고,
//! 센 횟수는 다시 켜도 그날 안에서는 이어진다. 손절은 AI를 부르지 않으므로 그대로 나간다.
//!
//! 논의가 열렸는지는 시연용 가짜 AI가 받은 글의 수로 본다(논의가 열리면 AI가 불린다).

use price_inha::NOTICES_PATH;

use crate::common::{Bench, HYNIX, Plan, STOP, WATCHED, get, kst, monday, plus, tuesday};

/// 상한에 닿았을 때의 알림 글(지시서 글자 그대로).
const CAPPED: &str = "오늘 AI 호출 상한에 닿아 논의를 쉽니다";

/// 논의 하나를 열어 본다(결과는 보지 않는다: 열렸는지는 AI가 불렸는지로 센다).
async fn discuss(bench: &Bench, topic: &str) {
    let _outcome = bench.crew().discuss(WATCHED, topic).await;
}

/// 지금까지 시연용 가짜 AI가 받은 글의 수.
fn calls(bench: &Bench) -> usize {
    bench.prompts().len()
}

/// 알림 목록 가운데 상한에 닿았다는 것들(제목 + 본문).
async fn capped_notices(bench: &Bench) -> Vec<String> {
    let reply = get(bench.addr(), &format!("{NOTICES_PATH}?after=0&max=200")).await;
    assert_eq!(reply.status, 200, "{}", reply.text);
    let list = reply.body.as_array().cloned().unwrap_or_default();
    list.iter()
        .map(|notice| format!("{} {}", notice["title"], notice["body"]))
        .filter(|text| text.contains(CAPPED))
        .collect()
}

// 검증: E30 E34 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한_2로_켜면_세_번째_논의가_열리지_않고_알림이_한_번_가고_한국_자정_뒤_다시_열린다() {
    let bench = Plan::demo().ai_limit(2).start(kst(monday(), 23, 50)).await;

    discuss(&bench, "첫째").await;
    let after_first = calls(&bench);
    assert!(after_first > 0, "첫 논의는 AI를 부른다");
    discuss(&bench, "둘째").await;
    let after_second = calls(&bench);
    assert!(after_second > after_first, "둘째 논의도 연다");
    assert!(
        capped_notices(&bench).await.is_empty(),
        "상한 전에는 알리지 않는다"
    );

    discuss(&bench, "셋째").await;
    assert_eq!(
        calls(&bench),
        after_second,
        "상한에 닿으면 세 번째 논의는 AI를 부르지 않는다"
    );
    assert_eq!(
        capped_notices(&bench).await.len(),
        1,
        "사용자에게 한 번 알린다"
    );
    discuss(&bench, "넷째").await;
    assert_eq!(calls(&bench), after_second, "그날 남은 논의는 모두 쉰다");
    assert_eq!(
        capped_notices(&bench).await.len(),
        1,
        "같은 날 알림은 한 번이다"
    );

    // 한국 시각 자정이 지나면 센 수가 새로 시작해 다시 열린다.
    bench.set(kst(tuesday(), 0, 10));
    discuss(&bench, "다음 날 첫째").await;
    assert!(calls(&bench) > after_second, "다음 날은 다시 논의를 연다");
    bench.close().await;
}

// 검증: E30 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 센_호출_수는_저장돼_다시_켜도_그날_안에서는_이어지고_다음_날은_새로_센다() {
    let bench = Plan::demo().ai_limit(2).start(kst(monday(), 10, 0)).await;
    discuss(&bench, "첫째").await;
    discuss(&bench, "둘째").await;
    assert!(calls(&bench) > 0);

    let bench = bench.reopen(plus(kst(monday(), 10, 0), 60)).await;
    discuss(&bench, "다시 켠 뒤").await;
    assert_eq!(
        calls(&bench),
        0,
        "다시 켜도 그날 센 수가 이어져 논의가 열리지 않는다"
    );

    let bench = bench.reopen(kst(tuesday(), 10, 0)).await;
    discuss(&bench, "다음 날").await;
    assert!(calls(&bench) > 0, "다음 날은 새로 센다");
    bench.close().await;
}

// 검증: E30 N5
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한을_비우면_논의가_몇_번이든_열리고_알림도_없다() {
    let bench = Plan::demo().start(kst(monday(), 10, 0)).await;
    let mut seen = 0;
    for round in 0..5 {
        discuss(&bench, &format!("{round}번째")).await;
        assert!(calls(&bench) > seen, "{round}번째 논의도 열린다");
        seen = calls(&bench);
    }
    assert!(
        capped_notices(&bench).await.is_empty(),
        "상한이 없으니 알리지 않는다"
    );
    bench.close().await;
}

// 검증: E34 E30
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 상한에_닿아도_손절은_그대로_나간다() {
    let bench = Plan::demo().ai_limit(1).start(kst(monday(), 10, 0)).await;
    bench.ready().await;
    discuss(&bench, "첫째").await;
    let used = calls(&bench);
    discuss(&bench, "둘째").await;
    assert_eq!(calls(&bench), used, "상한에 닿았다");

    let held = bench.holding(HYNIX).await;
    bench.quote(HYNIX, STOP - 1_000, plus(bench.now(), 1)).await;
    assert!(
        !bench.sells(HYNIX).await.is_empty(),
        "손절은 AI를 부르지 않으므로 상한과 무관하게 나간다: {held:?}"
    );
    bench.close().await;
}
