//! 하루 정리 회고: 성적이 나쁜 지침을 내리고 이벤트를 남긴다.

use pi_journal::{Guideline, GuidelineBook, GuidelineKind, GuidelineState};
use price_inha::{KIND_GUIDELINE, TEAM_GUIDELINES_FILE};
use serde_json::{Value, json};

use crate::common::{Bench, events};

fn guideline(id: &str, text: &str, uses: u32, wins: u32) -> Guideline {
    Guideline {
        id: id.into(),
        key: format!("lesson:{id}"),
        kind: GuidelineKind::Manage,
        text: text.into(),
        when: "사기 판단을 할 때".into(),
        symbol: None,
        basis: vec!["memo:1".into()],
        uses,
        wins,
        losses: uses - wins,
        blocks: 0,
        created_at: 1,
        updated_at: 1,
        last_used_day: 0,
        state: GuidelineState::Active,
        retired_why: None,
    }
}

// 검증: V100 R8 (하루 정리 회고: 쓰임이 충분한데 익절 비율이 낮은 지침을 내리고 이벤트를 남긴다)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 하루_정리_회고는_성적이_나쁜_지침을_내린다() {
    let bench = Bench::start().await;
    let book = GuidelineBook {
        guidelines: vec![
            guideline("g1", "장 막판에는 새로 들어가지 않는다", 6, 1),
            guideline("g2", "실적 발표 뒤 첫날은 지켜본다", 6, 4),
        ],
        next_id: 2,
        ..GuidelineBook::default()
    };
    std::fs::write(
        bench.home().join(TEAM_GUIDELINES_FILE),
        serde_json::to_vec(&book).unwrap(),
    )
    .unwrap();
    bench.close_day().await;

    let retired: Vec<Value> = events(&bench.store(), KIND_GUIDELINE)
        .into_iter()
        .filter(|event| event["action"] == json!("retired"))
        .collect();
    assert_eq!(retired.len(), 1, "{retired:?}");
    assert_eq!(retired[0]["id"], json!("g1"));
    assert_eq!(retired[0]["why"], json!("poor_win_rate"));
    let body = bench.guidelines().await;
    assert_eq!(body["active"][0]["id"], json!("g2"));
    assert_eq!(body["active"][0]["win_rate_pct"], json!(66));
    assert_eq!(body["retired"][0]["id"], json!("g1"));
    assert_eq!(body["retired"][0]["retired_why"], json!("poor_win_rate"));
    bench.close().await;
}
