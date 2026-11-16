//! 담당 도구와 팀 조립의 기본값: 지식 검색을 담당 도구로 등록하고(T16), 공식 조립의 종목 →
//! DART 고유번호 표를 채운다.

use pi_agent::Role;
use pi_data::CorpCode;
use pi_market::Symbol;
use price_inha::{DemoTurn, KNOWLEDGE_TOOL, TeamWiring};

use crate::common::{Bench, ENTRY, HYNIX, kst, monday};

// 검증: T16 V22 V23
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당이_지식_검색_도구를_부르면_검색_결과를_받는다() {
    let bench = Bench::start(kst(monday(), 10, 0)).await;
    let call = format!(
        r#"{{"action": "{KNOWLEDGE_TOOL}", "args": {{"mode": "keyword", "query": "실적 개선"}}}}"#
    );
    bench.script(DemoTurn::Talk(Role::Scout), &call);
    bench.quote(HYNIX, ENTRY, bench.now()).await;
    // 같은 답을 되풀이해 논의는 끝나지 못할 수 있다. 도구 결과가 담당에게 갔는지만 본다.
    let _ = bench.crew().discuss(HYNIX, "자기주식취득결정 공시").await;
    let prompts = bench.ai().prompts();
    let head = format!("[도구 결과 {KNOWLEDGE_TOOL}]");
    assert!(
        prompts
            .iter()
            .any(|p| p.contains(&head) && p.contains("\"results\"")),
        "검색 결과가 담당에게 간다: {prompts:?}"
    );
    let unknown = format!("그런 도구가 없습니다: {KNOWLEDGE_TOOL}");
    assert!(
        prompts.iter().all(|p| !p.contains(&unknown)),
        "모르는 도구가 아니다"
    );
    bench.close().await;
}

// 검증: E32 V31
#[test]
fn 공식_조립의_고유번호_표에_큰_종목이_있다() {
    let team = TeamWiring::official();
    for (symbol, corp) in [("005930", "00126380"), ("000660", "00164779")] {
        assert_eq!(
            team.corp_codes.get(&Symbol::new(symbol)),
            Some(&CorpCode::parse(corp).unwrap()),
            "{symbol}의 고유번호"
        );
    }
}
