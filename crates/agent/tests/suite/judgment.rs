//! 점수 턴 · 합의 점수 · 팀장 판단(만드는 것 3 · 5번, 결정 15번, V3 · M2 · E27 · E32 · P9).

use pi_agent::{Decision, Role, WatchReason};
use pi_ai::AiError;
use pi_store::{Amount, Rationale, RoleScore, SourceRef, TradeState, UnixMillis};
use serde_json::json;

use crate::common::{
    Env, NOW, Step, leader_buy, of_kind, request, say, score_reply, script_after_discussion,
    watch_codes,
};

fn scored(role: &str, score: i32, reason: &str) -> RoleScore {
    RoleScore {
        role: role.into(),
        score,
        reason: reason.into(),
    }
}

/// 점수 턴 대본: 70 · 80 · 60.
fn script_scores(env: &Env) {
    env.ai
        .script(Role::Scout, vec![Step::Say(score_reply(70, "a"))]);
    env.ai
        .script(Role::Strategy, vec![Step::Say(score_reply(80, "b"))]);
    env.ai
        .script(Role::Risk, vec![Step::Say(score_reply(60, "c"))]);
}

/// 논의는 뉴스 공시 시세 담당 한 글로 끝난다.
fn quiet_discussion(env: &Env) {
    env.ai.script(Role::Scout, vec![say("공시 요약을 남긴다")]);
}

// 검증: V3 M2 P9 (담당 점수와 팀장 판단이 매매 기록에 남고 판단됨이 된다)
#[tokio::test]
async fn 점수와_판단이_매매_기록에_남는다() {
    let env = Env::new();
    let trade = env.trade();
    quiet_discussion(&env);
    script_after_discussion(&env.ai);
    let out = env.team().discuss(request(trade)).await.unwrap();
    let scores = vec![
        scored("scout", 70, "공시가 좋다"),
        scored("strategy", 80, "추세 상승"),
        scored("risk", 60, "비중 작게"),
    ];
    assert_eq!(out.decision, Decision::Buy);
    assert_eq!(out.scores, scores);
    assert_eq!(out.consensus, Some(210));
    let saved = env.store.get_trade(trade).unwrap();
    assert_eq!(saved.state, TradeState::Judged);
    assert_eq!(saved.scores, scores);
    assert_eq!(saved.consensus_score, Some(210));
    assert_eq!(saved.decided_at, Some(NOW));
    let rationale = Rationale {
        hypothesis: Some("실적 개선".into()),
        trigger: Some("공시".into()),
        entry: Some(Amount(70_000)),
        stop: Some(Amount(65_000)),
        target: Some(Amount(80_000)),
        expected_value: Some(Amount(3_000)),
    };
    assert_eq!(saved.rationale, rationale);
    let known = UnixMillis(NOW.0 - 1_000);
    let sources = vec![
        SourceRef {
            name: "공시".into(),
            known_at: known,
        },
        SourceRef {
            name: "시세".into(),
            known_at: known,
        },
    ];
    assert_eq!(saved.sources, sources);
}

// 검증: V3 결정 15 (팀장 판단은 방 로그에, 점수는 방 로그와 따로)
#[tokio::test]
async fn 점수는_방_로그와_따로_저장한다() {
    let env = Env::new();
    let trade = env.trade();
    quiet_discussion(&env);
    script_after_discussion(&env.ai);
    env.team().discuss(request(trade)).await.unwrap();
    let events = env.room("room-1");
    for event in &events {
        assert!(event.payload.get("score").is_none(), "{}", event.kind);
        assert!(
            !event.payload.to_string().contains("추세 상승"),
            "{}",
            event.kind
        );
    }
    let leader = of_kind(&events, "message.member")
        .into_iter()
        .find(|e| e.actor_id == "leader")
        .unwrap();
    assert_eq!(leader.payload["decision"], json!("buy"));
    assert!(env.session_text("strategy").contains("추세 상승"));
    assert!(watch_codes(&events).is_empty());
}

// 검증: E27 (합의 점수가 기준을 넘지 못하면 지켜보기, 같아도 못 넘음)
#[tokio::test]
async fn 합의_점수가_기준을_넘지_못하면_지켜본다() {
    for required in [210, 300] {
        let env = Env::new();
        let trade = env.trade();
        quiet_discussion(&env);
        script_after_discussion(&env.ai);
        let mut req = request(trade);
        req.consensus_required = required;
        let out = env.team().discuss(req).await.unwrap();
        let reason = WatchReason::ConsensusBelow {
            consensus: 210,
            required,
        };
        assert_eq!(out.decision, Decision::Watch(reason));
        assert_eq!(out.consensus, Some(210));
        assert_eq!(out.order, None);
        assert_eq!(env.ai.calls(Role::Leader), 0);
        assert_eq!(env.ai.calls(Role::Trader), 0);
        assert!(env.tools.drafts().is_empty());
        let saved = env.store.get_trade(trade).unwrap();
        assert_eq!(saved.state, TradeState::Discussing);
        assert_eq!(saved.consensus_score, None);
        assert_eq!(watch_codes(&env.room("room-1")), ["consensus_below"]);
    }
}

// 검증: E6 E27 결정 15 (점수 턴에서 담당 하나가 실패하면 합의 불가로 지켜보기, 표로)
#[tokio::test]
async fn 점수_턴이_하나라도_실패하면_지켜본다() {
    let cases = [
        ("점수 JSON 없음", say("점수는 비밀")),
        ("범위 밖", Step::Say(score_reply(101, "과함"))),
        ("공급자 오류", Step::Fail(AiError::Connection)),
    ];
    for (why, bad) in cases {
        let env = Env::new();
        let trade = env.trade();
        quiet_discussion(&env);
        env.ai
            .script(Role::Scout, vec![Step::Say(score_reply(70, "좋다"))]);
        env.ai.script(Role::Strategy, vec![bad]);
        env.ai
            .script(Role::Risk, vec![Step::Say(score_reply(60, "작게"))]);
        env.ai.script(Role::Leader, vec![Step::Say(leader_buy())]);
        let out = env.team().discuss(request(trade)).await.unwrap();
        let reason = WatchReason::RoleUnavailable(Role::Strategy);
        assert_eq!(out.decision, Decision::Watch(reason), "{why}");
        assert_eq!(out.consensus, None, "{why}");
        assert_eq!(out.order, None, "{why}");
        assert_eq!(env.ai.calls(Role::Leader), 0, "{why}");
        let saved = env.store.get_trade(trade).unwrap();
        assert_eq!(saved.state, TradeState::Discussing, "{why}");
        assert_eq!(
            watch_codes(&env.room("room-1")),
            ["role_unavailable"],
            "{why}"
        );
    }
}

// 검증: E32 (필요한 자료가 비면 지켜보기, 주문 안 없음)
#[tokio::test]
async fn 자료가_비면_지켜본다() {
    let env = Env::new();
    let trade = env.trade();
    quiet_discussion(&env);
    script_after_discussion(&env.ai);
    let mut req = request(trade);
    req.data[1].content = None;
    let out = env.team().discuss(req).await.unwrap();
    let reason = WatchReason::MissingData(vec!["시세".into()]);
    assert_eq!(out.decision, Decision::Watch(reason));
    assert_eq!(out.order, None);
    assert!(env.tools.drafts().is_empty());
    assert_eq!(env.ai.calls(Role::Leader), 0);
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Discussing
    );
    assert_eq!(watch_codes(&env.room("room-1")), ["missing_data"]);
}

// 검증: E27 (팀장이 지켜보기로 정하면 이유와 다시 볼 조건이 남는다)
#[tokio::test]
async fn 팀장이_지켜보기로_정할_수_있다() {
    let env = Env::new();
    let trade = env.trade();
    quiet_discussion(&env);
    script_scores(&env);
    let leader_watch =
        "{\"decision\": \"watch\", \"reason\": \"변동성 큼\", \"revisit\": \"거래량 회복\"}";
    env.ai.script(Role::Leader, vec![say(leader_watch)]);
    let out = env.team().discuss(request(trade)).await.unwrap();
    let reason = WatchReason::LeaderWatch {
        reason: "변동성 큼".into(),
        revisit: "거래량 회복".into(),
    };
    assert_eq!(out.decision, Decision::Watch(reason));
    assert_eq!(out.order, None);
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Discussing
    );
    assert_eq!(watch_codes(&env.room("room-1")), ["leader_watch"]);
}

// 검증: E6 (팀장 판단의 모양이 틀리면 팀장 응답 실패로 지켜보기, 표로)
#[tokio::test]
async fn 팀장_판단이_틀리면_지켜본다() {
    let cases = [
        ("판단 없음", "사는 게 좋겠다"),
        ("모르는 판단", "{\"decision\": \"yolo\"}"),
        (
            "금액이 글자",
            "{\"decision\": \"buy\", \"hypothesis\": \"a\", \"trigger\": \"b\", \
             \"entry\": \"칠만\", \"stop\": 1, \"target\": 2, \"expected_value\": 3}",
        ),
    ];
    for (why, reply) in cases {
        let env = Env::new();
        let trade = env.trade();
        quiet_discussion(&env);
        script_scores(&env);
        env.ai.script(Role::Leader, vec![say(reply)]);
        let out = env.team().discuss(request(trade)).await.unwrap();
        let reason = WatchReason::RoleUnavailable(Role::Leader);
        assert_eq!(out.decision, Decision::Watch(reason), "{why}");
        assert_eq!(out.order, None, "{why}");
        let saved = env.store.get_trade(trade).unwrap();
        assert_eq!(saved.state, TradeState::Discussing, "{why}");
    }
}

// 검증: P9 (사기인데 근거 칸이 비면 지켜보기, 판단을 저장하지 않는다)
#[tokio::test]
async fn 근거가_빈_사기는_지켜본다() {
    let env = Env::new();
    let trade = env.trade();
    quiet_discussion(&env);
    script_scores(&env);
    let no_stop = "{\"decision\": \"buy\", \"hypothesis\": \"실적\", \"trigger\": \"  \", \
                   \"entry\": 70000, \"target\": 80000, \"expected_value\": 3000}";
    env.ai.script(Role::Leader, vec![say(no_stop)]);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::RationaleIncomplete)
    );
    assert_eq!(out.order, None);
    assert_eq!(env.ai.calls(Role::Trader), 0);
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Discussing
    );
    assert_eq!(watch_codes(&env.room("room-1")), ["rationale_incomplete"]);
}
