//! 기능 L(결정 30): 판단 글의 "팀 지침" 블록 — 브리핑의 정해진 자리에 들고, 개수 · 글자 상한을 지키고, 지침이
//! 없으면 블록이 없다. 번호 V103은 이 차수에서 설계서에 더할 제안 번호다.

use pi_agent::{
    DiscussionOptions, GUIDELINE_BLOCK_NOTE, GUIDELINE_BLOCK_TITLE, GUIDELINE_CHARS_MAX,
    GUIDELINE_LINES_MAX, NoticeNote, Role,
};
use pi_store::{TradeId, UnixMillis};

use crate::common::{Env, MASK, NOW, SECRET_MARK, model, request, say, script_after_discussion};

/// 담당 셋이 한 번씩 말하고 점수 · 팀장 · 매매 담당이 이어지는 대본.
fn scripted(env: &Env) -> TradeId {
    let trade = env.trade();
    env.ai
        .script(Role::Scout, vec![say("공시 요약. @strategy @risk 봐 줘")]);
    env.ai.script(Role::Strategy, vec![say("추세가 좋다")]);
    env.ai.script(Role::Risk, vec![say("비중을 작게")]);
    script_after_discussion(&env.ai);
    trade
}

/// 그 담당이 받은 요청마다 모든 글을 이어 붙인 것.
fn texts(env: &Env, role: Role) -> Vec<String> {
    let name = model(role);
    env.ai
        .requests()
        .iter()
        .filter(|r| r.model == name)
        .map(|r| {
            r.messages
                .iter()
                .map(|m| m.content.clone())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

fn all_texts(env: &Env) -> Vec<String> {
    env.ai
        .requests()
        .iter()
        .flat_map(|r| r.messages.iter().map(|m| m.content.clone()))
        .collect()
}

async fn discuss(env: &Env, options: DiscussionOptions) {
    let trade = scripted(env);
    env.team()
        .discuss_with(request(trade), options)
        .await
        .unwrap();
}

// 검증: V103 R8 (지침을 넘기면 담당 · 팀장의 판단 글에 "팀 지침" 블록이 자료 · 공지 뒤에 받은 순서로 든다)
#[tokio::test]
async fn 지침을_넘기면_담당과_팀장의_판단_글에_팀_지침_블록이_든다() {
    let env = Env::new();
    let options = DiscussionOptions::default()
        .with_notices(vec![NoticeNote {
            text: "공지 하나".into(),
            sent_at: UnixMillis(NOW.0 - 10),
        }])
        .with_guidelines(vec![
            "[피하기] 추격 매수를 하지 않는다 (언제: 급등 뒤)".into(),
            "[들어가기] 자사주 매입 공시에는 들어간다 (언제: 새 공시)".into(),
        ]);
    discuss(&env, options).await;
    assert_eq!(GUIDELINE_BLOCK_TITLE, "[팀 지침]");
    assert_eq!(
        GUIDELINE_BLOCK_NOTE,
        "지침은 참고다. 한도 · 금지는 시스템이 따로 막는다"
    );
    for role in [Role::Scout, Role::Strategy, Role::Risk, Role::Leader] {
        let first = &texts(&env, role)[0];
        let data = first.find("시세 내용").unwrap();
        let notice = first.find("공지 하나").unwrap();
        let block = first
            .find(GUIDELINE_BLOCK_TITLE)
            .unwrap_or_else(|| panic!("{role:?}: 블록이 있어야 한다\n{first}"));
        let note = first.find(GUIDELINE_BLOCK_NOTE).unwrap();
        let avoid = first.find("추격 매수를 하지 않는다").unwrap();
        let enter = first.find("자사주 매입 공시에는").unwrap();
        assert!(data < block && notice < block, "{role:?}: 자료 · 공지 뒤");
        assert!(
            block < note && note < avoid && avoid < enter,
            "{role:?}: 머리 · 한 줄 · 받은 순서"
        );
    }
}

// 검증: V103 R8 (지침이 없으면 블록이 없다)
#[tokio::test]
async fn 지침이_없으면_블록이_없다() {
    let env = Env::new();
    discuss(
        &env,
        DiscussionOptions::default().with_guidelines(Vec::new()),
    )
    .await;
    assert!(
        all_texts(&env)
            .iter()
            .all(|text| !text.contains(GUIDELINE_BLOCK_TITLE)
                && !text.contains(GUIDELINE_BLOCK_NOTE)),
        "블록 머리 · 한 줄이 어디에도 없다"
    );
}

// 검증: V103 R8 (받은 줄이 많아도 개수 상한 안에서만 든다)
#[tokio::test]
async fn 지침_줄은_개수_상한_안에서만_든다() {
    assert_eq!(GUIDELINE_LINES_MAX, 7);
    let env = Env::new();
    let lines: Vec<String> = (0..10).map(|n| format!("지침줄{n:02} 내용")).collect();
    discuss(&env, DiscussionOptions::default().with_guidelines(lines)).await;
    let first = &texts(&env, Role::Scout)[0];
    for n in 0..7 {
        assert!(first.contains(&format!("지침줄{n:02}")), "{n}번 줄이 든다");
    }
    for n in 7..10 {
        assert!(
            !first.contains(&format!("지침줄{n:02}")),
            "{n}번 줄은 들지 않는다"
        );
    }
}

// 검증: V103 R8 (지침 줄의 글자 합은 상한을 넘지 않는다: 넘는 줄은 건너뛴다)
#[tokio::test]
async fn 지침_줄의_글자_합은_상한을_넘지_않는다() {
    assert_eq!(GUIDELINE_CHARS_MAX, 1_200);
    let env = Env::new();
    let long = |tag: &str| format!("{tag}{}", "가".repeat(500));
    let lines = vec![
        long("첫긴줄"),
        long("둘긴줄"),
        long("셋긴줄"),
        "짧은줄 하나".into(),
    ];
    discuss(&env, DiscussionOptions::default().with_guidelines(lines)).await;
    let first = &texts(&env, Role::Scout)[0];
    let block = &first[first.find(GUIDELINE_BLOCK_TITLE).unwrap()..];
    assert!(block.contains("첫긴줄") && block.contains("둘긴줄"));
    assert!(!block.contains("셋긴줄"), "넘는 줄은 건너뛴다");
    assert!(block.contains("짧은줄 하나"), "그 뒤의 짧은 줄은 든다");
}

// 검증: V103 E38 (지침 글의 비밀 표시는 가려서 나간다)
#[tokio::test]
async fn 지침_글의_비밀_표시는_가려서_나간다() {
    let env = Env::new();
    let lines = vec![format!("[운용] 열쇠 {SECRET_MARK} 쓰지 않는다 (언제: 늘)")];
    discuss(&env, DiscussionOptions::default().with_guidelines(lines)).await;
    assert!(
        all_texts(&env)
            .iter()
            .all(|text| !text.contains(SECRET_MARK))
    );
    assert!(texts(&env, Role::Scout)[0].contains(&format!("열쇠 {MASK} 쓰지 않는다")));
}
