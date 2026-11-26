//! 4번: 시연용 가짜 AI의 사무실 채팅 답. 영어 id(`strategy 담당입니다 …`) 대신 표시 이름(전략 · 리스크 …)으로 말하고,
//! 물음을 한 줄 되받으며, 시연 답임을 알 수 있게 "(시연)"을 붙인다. 바깥으로 아무것도 보내지 않는다.

use pi_ai::{AiProvider, CompletionRequest, Message, Role as Speaker};
use price_inha::{DemoAi, OFFICE_ROSTER};

/// 사무실 채팅 턴의 머리(`[사무실 채팅]`)를 단 물음으로 그 담당에게 묻는다.
async fn ask(role_id: &str, question: &str) -> String {
    let request = CompletionRequest {
        model: "demo".to_string(),
        system: Some(format!("너는 투자 팀의 한 사람이다.\n담당 id: {role_id}\n")),
        messages: vec![Message {
            role: Speaker::User,
            content: format!("[사무실 채팅] {question}"),
        }],
        max_tokens: 256,
    };
    DemoAi::new()
        .complete(request)
        .await
        .expect("시연 답이 온다")
        .text
}

/// 담당 id → 사무실 표시 이름(뉴스 공시 시세 담당의 id는 `scout`, 사무실 명단은 `news`).
fn display_name(role_id: &str) -> &'static str {
    let office_id = if role_id == "scout" { "news" } else { role_id };
    OFFICE_ROSTER
        .iter()
        .find(|(id, _)| *id == office_id)
        .map(|(_, name)| *name)
        .unwrap()
}

// 검증: D14 C18
#[tokio::test]
async fn 시연_채팅_답은_표시_이름으로_물음을_되받고_시연이라고_적는다() {
    for role_id in ["scout", "strategy", "risk", "review", "trader", "leader"] {
        let reply = ask(role_id, "오늘 시장 어때요?").await;
        let name = display_name(role_id);
        assert!(
            reply.starts_with(&format!("{name}입니다.")),
            "표시 이름으로 말한다: {reply}"
        );
        assert!(
            reply.contains("'오늘 시장 어때요?'"),
            "물음을 한 줄 되받는다: {reply}"
        );
        assert!(reply.contains("(시연)"), "시연 답임을 알린다: {reply}");
        assert!(!reply.contains(role_id), "영어 id가 나오지 않는다: {reply}");
        assert!(!reply.contains('\n'), "한 줄이다: {reply}");
    }
}

// 검증: D14
#[tokio::test]
async fn 긴_물음은_줄여서_되받고_빈_물음은_되받지_않는다() {
    let long = "가".repeat(300);
    let reply = ask("risk", &long).await;
    assert!(reply.starts_with("리스크입니다."), "{reply}");
    assert!(!reply.contains(&long), "긴 물음을 통째로 되받지 않는다");
    assert!(reply.chars().count() < 200, "답이 짧다: {reply}");
    assert!(reply.contains("(시연)"), "{reply}");

    let reply = ask("leader", "   ").await;
    assert!(reply.starts_with("팀장입니다."), "{reply}");
    assert!(!reply.contains("''"), "빈 물음을 되받지 않는다: {reply}");
    assert!(reply.contains("(시연)"), "{reply}");
}
