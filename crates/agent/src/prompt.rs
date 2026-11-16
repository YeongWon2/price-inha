//! 담당에게 보내는 글: 시스템 글 세 층(공통 · 담당 SOUL · 팀 규칙)과 턴마다의 입력 글.

use pi_store::{RoleScore, Trade};

use crate::discussion::DiscussionRequest;
use crate::role::{Role, RoleConfig};

/// 공통 층: 행동 JSON 형식(결정 12번).
const COMMON: &str = "너는 가격인하 투자 팀의 담당이다.\n\
도구를 부르려면 답 글에 JSON 객체 {\"action\": \"<도구 이름>\", \"args\": { ... }}를 쓴다. \
한 답에 여러 개를 써도 된다. 도구 결과는 다음 글로 받는다.\n\
도구를 더 부르지 않을 때는 행동 JSON 없이 최종 답만 쓴다.";

/// 팀 규칙 층.
const TEAM_RULES: &str = "[팀 규칙]\n\
- 너의 말은 승인이 아니다. 주문을 내거나 승인하는 일은 사람과 규칙 엔진이 한다.\n\
- 수량과 가격은 규칙 엔진이 정한다. 네가 정하지 않는다.\n\
- 자료가 모자라면 모자란다고 말한다. 지어내지 않는다.";

/// 깨진 행동 JSON 뒤에 붙이는 안내.
pub(crate) const MALFORMED_NUDGE: &str = "행동 JSON이 깨졌습니다. \
{\"action\": \"<도구 이름>\", \"args\": { ... }} 모양으로 다시 쓰거나, 행동 없이 최종 답을 쓰세요.";

/// 시스템 글: 공통 · 담당 SOUL · 팀 규칙.
pub(crate) fn system(config: &RoleConfig, tools: &[String]) -> String {
    let tools = if tools.is_empty() {
        "(없음)".to_string()
    } else {
        tools.join(", ")
    };
    format!(
        "{COMMON}\n담당 id: {id}\n부를 수 있는 도구: {tools}\n\n[담당]\n{soul}\n\n{TEAM_RULES}",
        id = config.role.id(),
        soul = config.soul,
    )
}

/// 도구 결과 글.
pub(crate) fn tool_result(name: &str, content: &str) -> String {
    format!("[도구 결과 {name}]\n{content}")
}

/// 모르는 도구의 결과 글.
pub(crate) fn unknown_tool(name: &str) -> String {
    format!("오류: 그런 도구가 없습니다: {name}")
}

/// 논의 방에 처음 들어온 담당에게 주는 안내(계기 · 자료 · 참가 담당).
pub(crate) fn briefing(request: &DiscussionRequest, symbol: &str) -> String {
    let members: Vec<String> = request
        .members
        .iter()
        .map(|role| format!("@{}", role.id()))
        .collect();
    let mut out = format!(
        "[팀 논의] 방 {room} · 매매 {trade} · 종목 {symbol}\n계기: {topic}\n참가: {members}\n자료:\n",
        room = request.room,
        trade = request.trade.0,
        topic = request.topic,
        members = members.join(" "),
    );
    for item in &request.data {
        let content = item.content.as_deref().unwrap_or("(없음)");
        out.push_str(&format!("- {}: {}\n", item.name, content));
    }
    out
}

/// 논의 턴의 입력 글.
pub(crate) fn discussion_turn(
    briefing: Option<&str>,
    round: u32,
    unseen: &[(Role, String)],
) -> String {
    let mut out = briefing.map(|text| format!("{text}\n")).unwrap_or_default();
    if !unseen.is_empty() {
        out.push_str("[방 글]\n");
        out.push_str(&transcript(unseen));
    }
    out.push_str(&format!(
        "\n{round}라운드다. 의견을 짧게 쓴다. 다른 담당이 봐야 하면 @<id>로 부른다. \
         보탤 것이 없으면 (pass)라고만 쓴다."
    ));
    out
}

/// 점수 턴의 입력 글.
pub(crate) fn score_turn() -> String {
    "[점수] 논의를 바탕으로 이 매매를 0~100 점수로 매긴다. \
     답은 JSON 객체 하나다: {\"score\": <정수>, \"reason\": \"<근거>\"}"
        .to_string()
}

/// 팀장 판단 턴의 입력 글.
pub(crate) fn leader_turn(
    briefing: &str,
    said: &[(Role, String)],
    scores: &[RoleScore],
    consensus: i32,
) -> String {
    let mut out = format!("{briefing}\n[논의]\n{}\n[담당 점수]\n", transcript(said));
    for scored in scores {
        out.push_str(&format!(
            "- {}: {} ({})\n",
            scored.role, scored.score, scored.reason
        ));
    }
    out.push_str(&format!(
        "합의 점수: {consensus}\n\n판단을 JSON 객체 하나로 답한다.\n\
         사기 · 팔기: {{\"decision\": \"buy\"|\"sell\", \"hypothesis\": \"<가설>\", \
         \"trigger\": \"<계기>\", \"entry\": <정수>, \"stop\": <정수>, \"target\": <정수>, \
         \"expected_value\": <정수>}}\n\
         지켜보기: {{\"decision\": \"watch\", \"reason\": \"<이유>\", \"revisit\": \"<다시 볼 조건>\"}}"
    ));
    out
}

/// 매매 담당의 주문 안 턴의 입력 글.
pub(crate) fn order_turn(trade: &Trade) -> String {
    format!(
        "[주문 안] 판단된 매매 {id} · 종목 {symbol}의 주문 안을 만든다. \
         수량과 가격은 규칙 엔진이 정하므로 적지 않는다.\n\
         답은 JSON 객체 하나다: {{\"side\": \"buy\", \"note\": \"<설명>\"}}",
        id = trade.id.0,
        symbol = trade.symbol,
    )
}

fn transcript(said: &[(Role, String)]) -> String {
    said.iter()
        .map(|(role, text)| format!("@{}: {}\n", role.id(), text))
        .collect()
}
