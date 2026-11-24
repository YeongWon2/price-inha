//! 담당 의견 칸 · 팀장 요약 · 뒤집힐 조건 · 공지를 논의 자료에(11차, R6-4 R6-5 R17-2 M9 D13 V68 V69 V72).
//!
//! 점수 턴의 새 칸 이름은 `stance` · `conviction` · `opinion_reason` · `horizon_days`,
//! 팀장 턴의 새 칸 이름은 `summary` · `reversal`이다(지시서가 값만 정해 두어 이 이름으로 고른다).
use std::sync::{Arc, Mutex};

use pi_agent::{
    Decision, DiscussionOptions, NOTICE_SOURCE_NAME, NoticeNote, ProgressSignal, ProgressSink,
    Role, SignalError, WatchReason,
};
use pi_store::{
    Conviction, RoleOpinion, RoleScore, SourceRef, Stance, TradeId, TradeSide, TradeState,
    TradeSummary, UnixMillis,
};

use crate::common::{
    Env, MASK, NOW, SECRET_MARK, Step, leader_buy, model, request, say, score_reply, trader_buy,
};

/// 새 칸이 모두 든 점수 턴의 답 글. 보는 기간은 없을 수 있다.
fn opinion_reply(
    score: i32,
    reason: &str,
    stance: &str,
    conviction: &str,
    why: &str,
    horizon: Option<u32>,
) -> String {
    let horizon = horizon
        .map(|days| format!(", \"horizon_days\": {days}"))
        .unwrap_or_default();
    format!(
        "{{\"score\": {score}, \"reason\": \"{reason}\", \"stance\": \"{stance}\", \
         \"conviction\": \"{conviction}\", \"opinion_reason\": \"{why}\"{horizon}}}"
    )
}

/// 사기 판단 답에 칸을 더한 글(`extra`는 `, "칸": 값` 꼴).
fn leader_buy_with(extra: &str) -> String {
    format!("{}{extra}}}", leader_buy().trim_end_matches('}'))
}

fn leader_summary() -> String {
    leader_buy_with(
        ", \"summary\": \"실적이 좋다. 다만 과열이 걱정이다.\", \"reversal\": \"외국인이 팔면 뒤집힌다\"",
    )
}

fn the_summary() -> TradeSummary {
    TradeSummary {
        summary: "실적이 좋다. 다만 과열이 걱정이다.".into(),
        reversal: "외국인이 팔면 뒤집힌다".into(),
    }
}

/// 뉴스 공시 시세 · 전략 · 리스크 세 담당이 논의하는 요청(합의 기준 150).
fn script_three(env: &Env, scout: &str, strategy: &str, risk: &str, leader: &str) -> TradeId {
    let trade = env.trade();
    env.ai
        .script(Role::Scout, vec![say("공시 요약"), say(scout)]);
    env.ai.script(Role::Strategy, vec![say(strategy)]);
    env.ai.script(Role::Risk, vec![say(risk)]);
    env.ai.script(Role::Leader, vec![say(leader)]);
    env.ai
        .script(Role::Trader, vec![Step::Say(trader_buy("공시 뒤 사기"))]);
    trade
}

/// 세 담당이 새 칸을 모두 채운 대본(70 · 80 · 60, 합 210).
fn full_script(env: &Env, leader: &str) -> TradeId {
    script_three(
        env,
        &opinion_reply(70, "공시가 좋다", "bullish", "high", "공시 호재", Some(20)),
        &opinion_reply(80, "추세 상승", "bearish", "medium", "과열 부담", None),
        &opinion_reply(60, "비중 작게", "neutral", "low", "변동 큼", Some(5)),
        leader,
    )
}

fn opinion(
    role: Role,
    stance: Stance,
    conviction: Conviction,
    reason: &str,
    horizon_days: Option<u32>,
) -> RoleOpinion {
    RoleOpinion {
        role: role.id().to_string(),
        stance: Some(stance),
        conviction: Some(conviction),
        reason: reason.into(),
        horizon_days,
        abstained: false,
    }
}

fn roles_of(opinions: &[RoleOpinion]) -> Vec<String> {
    opinions.iter().map(|o| o.role.clone()).collect()
}

fn ids(roles: &[Role]) -> Vec<String> {
    roles.iter().map(|r| r.id().to_string()).collect()
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

fn notice(text: &str, ago: i64) -> NoticeNote {
    NoticeNote {
        text: text.into(),
        sent_at: UnixMillis(NOW.0 - ago),
    }
}

fn source(name: &str, at: UnixMillis) -> SourceRef {
    SourceRef {
        name: name.into(),
        known_at: at,
    }
}

// 검증: R6-4 R6-5 M9 D13 V68 V69 (새 칸이 있는 점수 · 팀장 답이 판단과 함께 저장소에 남는다)
#[tokio::test]
async fn 새_칸이_있으면_판단과_함께_담당별_의견과_팀장_요약이_남는다() {
    let env = Env::new();
    let trade = full_script(&env, &leader_summary());
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    let saved = env.store.get_trade(trade).unwrap();
    assert_eq!(saved.state, TradeState::Judged);
    let kept = env.store.trade_opinions(trade).unwrap();
    assert_eq!(
        kept.opinions,
        vec![
            opinion(
                Role::Scout,
                Stance::Bullish,
                Conviction::High,
                "공시 호재",
                Some(20)
            ),
            opinion(
                Role::Strategy,
                Stance::Bearish,
                Conviction::Medium,
                "과열 부담",
                None
            ),
            opinion(
                Role::Risk,
                Stance::Neutral,
                Conviction::Low,
                "변동 큼",
                Some(5)
            ),
        ]
    );
    assert_eq!(kept.summary, Some(the_summary()));
}

// 검증: D13 M9 (담당 점수는 그대로 — 담당 · 점수 · 근거만, 합의 점수도 같다)
#[tokio::test]
async fn 새_칸이_있어도_담당_점수와_합의_점수는_그대로다() {
    let env = Env::new();
    let trade = full_script(&env, &leader_summary());
    let out = env.team().discuss(request(trade)).await.unwrap();
    let scores = vec![
        RoleScore {
            role: "scout".into(),
            score: 70,
            reason: "공시가 좋다".into(),
        },
        RoleScore {
            role: "strategy".into(),
            score: 80,
            reason: "추세 상승".into(),
        },
        RoleScore {
            role: "risk".into(),
            score: 60,
            reason: "비중 작게".into(),
        },
    ];
    assert_eq!(out.scores, scores);
    assert_eq!(out.consensus, Some(210));
    let saved = env.store.get_trade(trade).unwrap();
    assert_eq!(saved.scores, scores);
    assert_eq!(saved.consensus_score, Some(210));
}

// 검증: R6-4 D13 V68 (옛 모양 점수 답은 점수만 받고 그 담당의 의견은 기록 없음)
#[tokio::test]
async fn 옛_모양_점수_답은_점수만_받고_그_담당의_의견은_없다() {
    let env = Env::new();
    let trade = script_three(
        &env,
        &score_reply(70, "공시가 좋다"),
        &opinion_reply(80, "추세 상승", "bearish", "medium", "과열 부담", None),
        &opinion_reply(60, "비중 작게", "neutral", "low", "변동 큼", Some(5)),
        &leader_summary(),
    );
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    assert_eq!(out.scores.len(), 3);
    assert_eq!(out.scores[0].score, 70);
    let kept = env.store.trade_opinions(trade).unwrap();
    assert_eq!(roles_of(&kept.opinions), ids(&[Role::Strategy, Role::Risk]));
    assert_eq!(kept.summary, Some(the_summary()));
}

// 검증: R6-4 D13 V68 (모두 옛 모양이어도 점수 턴은 실패가 아니고 팀장 요약만 남는다)
#[tokio::test]
async fn 모두_옛_모양_점수_답이어도_판단되고_요약만_남는다() {
    let env = Env::new();
    let trade = script_three(
        &env,
        &score_reply(70, "공시가 좋다"),
        &score_reply(80, "추세 상승"),
        &score_reply(60, "비중 작게"),
        &leader_summary(),
    );
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Buy);
    let kept = env.store.trade_opinions(trade).unwrap();
    assert!(kept.opinions.is_empty());
    assert_eq!(kept.summary, Some(the_summary()));
}

// 검증: R6-4 D13 V68 (틀린 방향 · 확신도 글자나 빠진 칸은 점수만 받고 그 담당의 의견은 없다)
#[tokio::test]
async fn 틀린_방향이나_확신도는_점수를_받고_의견만_없다() {
    let bad = [
        "\"stance\": \"up\", \"conviction\": \"high\"",
        "\"stance\": \"bullish\", \"conviction\": \"very high\"",
        "\"stance\": \"bullish\"",
        "\"conviction\": \"high\"",
        "\"stance\": 1, \"conviction\": \"high\"",
    ];
    for fragment in bad {
        let env = Env::new();
        let broken = format!(
            "{{\"score\": 70, \"reason\": \"공시가 좋다\", {fragment}, \
             \"opinion_reason\": \"이유\", \"horizon_days\": 10}}"
        );
        let trade = script_three(
            &env,
            &broken,
            &opinion_reply(80, "추세 상승", "bearish", "medium", "과열 부담", None),
            &opinion_reply(60, "비중 작게", "neutral", "low", "변동 큼", Some(5)),
            &leader_summary(),
        );
        let out = env.team().discuss(request(trade)).await.unwrap();
        assert_eq!(out.decision, Decision::Buy, "{fragment}");
        assert_eq!(out.scores[0].score, 70, "{fragment}");
        let kept = env.store.trade_opinions(trade).unwrap();
        assert_eq!(
            roles_of(&kept.opinions),
            ids(&[Role::Strategy, Role::Risk]),
            "{fragment}"
        );
    }
}

// 검증: R6-5 M9 V69 (팀장 답에 요약이나 뒤집힐 조건이 없으면 옛 길로 저장되고 매매는 판단된다)
#[tokio::test]
async fn 팀장_답에_두_칸이_다_없으면_옛_길로_저장되고_매매는_판단된다() {
    let replies = [
        leader_buy(),
        leader_buy_with(", \"summary\": \"실적이 좋다.\""),
        leader_buy_with(", \"reversal\": \"외국인이 팔면 뒤집힌다\""),
        leader_buy_with(", \"summary\": \"  \", \"reversal\": \"외국인이 팔면 뒤집힌다\""),
    ];
    for leader in replies {
        let env = Env::new();
        let trade = full_script(&env, &leader);
        let out = env.team().discuss(request(trade)).await.unwrap();
        assert_eq!(out.decision, Decision::Buy, "{leader}");
        assert!(out.order.is_some(), "{leader}");
        let saved = env.store.get_trade(trade).unwrap();
        assert_eq!(saved.state, TradeState::Judged, "{leader}");
        assert_eq!(
            env.store.trade_side(trade).unwrap(),
            Some(TradeSide::Buy),
            "{leader}"
        );
        let kept = env.store.trade_opinions(trade).unwrap();
        assert!(kept.opinions.is_empty(), "{leader}");
        assert_eq!(kept.summary, None, "{leader}");
    }
}

// 검증: R6-5 M9 V69 (팔기 판단도 방향과 함께 의견 · 요약이 한 쓰기로 남는다)
#[tokio::test]
async fn 팔기_판단도_의견과_요약을_남긴다() {
    let env = Env::new();
    let sell = leader_summary().replace("\"buy\"", "\"sell\"");
    let trade = full_script(&env, &sell);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(out.decision, Decision::Sell);
    assert_eq!(env.store.trade_side(trade).unwrap(), Some(TradeSide::Sell));
    let kept = env.store.trade_opinions(trade).unwrap();
    assert_eq!(kept.opinions.len(), 3);
    assert_eq!(kept.summary, Some(the_summary()));
}

// 검증: P9 R6-5 (근거 칸이 비면 요약이 있어도 지켜보기 — 판단도 의견도 남지 않는다)
#[tokio::test]
async fn 근거_칸이_비면_요약이_있어도_지켜보고_아무것도_남지_않는다() {
    let env = Env::new();
    let blank = leader_summary().replace("실적 개선", " ");
    let trade = full_script(&env, &blank);
    let out = env.team().discuss(request(trade)).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::RationaleIncomplete)
    );
    assert_eq!(
        env.store.get_trade(trade).unwrap().state,
        TradeState::Discussing
    );
    let kept = env.store.trade_opinions(trade).unwrap();
    assert!(kept.opinions.is_empty());
    assert_eq!(kept.summary, None);
}

// 검증: E38 R6-4 R6-5 (이유 · 요약 · 뒤집힐 조건 글은 가려서 남는다)
#[tokio::test]
async fn 이유와_요약과_뒤집힐_조건의_비밀_표시는_가려서_남는다() {
    let env = Env::new();
    let leader = leader_buy_with(&format!(
        ", \"summary\": \"요약 {SECRET_MARK} 끝\", \"reversal\": \"조건 {SECRET_MARK} 끝\""
    ));
    let trade = script_three(
        &env,
        &opinion_reply(
            70,
            "공시가 좋다",
            "bullish",
            "high",
            &format!("이유 {SECRET_MARK} 끝"),
            Some(20),
        ),
        &opinion_reply(80, "추세 상승", "bearish", "medium", "과열 부담", None),
        &opinion_reply(60, "비중 작게", "neutral", "low", "변동 큼", Some(5)),
        &leader,
    );
    env.team().discuss(request(trade)).await.unwrap();
    let kept = env.store.trade_opinions(trade).unwrap();
    assert_eq!(kept.opinions[0].reason, format!("이유 {MASK} 끝"));
    let summary = kept.summary.unwrap();
    assert_eq!(summary.summary, format!("요약 {MASK} 끝"));
    assert_eq!(summary.reversal, format!("조건 {MASK} 끝"));
}

// 검증: R6-4 R6-5 D13 (점수 턴 · 팀장 턴 지시 글에 새 칸의 모양이 적히고 기존 칸 설명은 그대로다)
#[tokio::test]
async fn 점수_턴과_팀장_턴의_지시_글에_새_칸의_모양이_적힌다() {
    let env = Env::new();
    let trade = full_script(&env, &leader_summary());
    env.team().discuss(request(trade)).await.unwrap();
    let score_turn = texts(&env, Role::Scout)
        .into_iter()
        .find(|t| t.contains("[점수]"))
        .unwrap();
    for needle in [
        "\"score\"",
        "\"reason\"",
        "\"stance\"",
        "\"conviction\"",
        "\"opinion_reason\"",
        "\"horizon_days\"",
        "bullish",
        "bearish",
        "neutral",
        "low",
        "medium",
        "high",
    ] {
        assert!(score_turn.contains(needle), "점수 턴에 {needle}이 없다");
    }
    let leader_turn = texts(&env, Role::Leader)
        .into_iter()
        .find(|t| t.contains("[담당 점수]"))
        .unwrap();
    for needle in [
        "\"decision\"",
        "\"hypothesis\"",
        "\"summary\"",
        "\"reversal\"",
    ] {
        assert!(leader_turn.contains(needle), "팀장 턴에 {needle}이 없다");
    }
}

// 검증: R17-2 V72 (공지를 넘기면 담당 브리핑의 자료 뒤에 "사용자 공지" 묶음이 보낸 시각 순서로 든다)
#[tokio::test]
async fn 공지를_넘기면_브리핑의_자료_뒤에_보낸_시각_순서로_든다() {
    let env = Env::new();
    let trade = full_script(&env, &leader_buy());
    let options = DiscussionOptions::default().with_notices(vec![
        notice("다음 주 실적 발표를 조심하라", 100),
        notice("손절 선은 꼭 지켜라", 5_000),
    ]);
    env.team()
        .discuss_with(request(trade), options)
        .await
        .unwrap();
    for role in [Role::Scout, Role::Strategy, Role::Risk] {
        let first = &texts(&env, role)[0];
        let group = first.find("사용자 공지").unwrap();
        let data = first.find("시세 내용").unwrap();
        let early = first.find("손절 선은 꼭 지켜라").unwrap();
        let late = first.find("다음 주 실적 발표를 조심하라").unwrap();
        assert!(data < group, "{role:?}: 공지는 자료 뒤에 든다");
        assert!(group < early && early < late, "{role:?}: 보낸 시각 순서");
    }
}

// 검증: E38 R17-2 (공지 글의 비밀 표시는 가려서 나간다)
#[tokio::test]
async fn 공지의_비밀_표시는_가려서_나간다() {
    let env = Env::new();
    let trade = full_script(&env, &leader_buy());
    let options = DiscussionOptions::default()
        .with_notices(vec![notice(&format!("열쇠 {SECRET_MARK} 쓰지 마라"), 100)]);
    env.team()
        .discuss_with(request(trade), options)
        .await
        .unwrap();
    let all: Vec<String> = env
        .ai
        .requests()
        .iter()
        .flat_map(|r| r.messages.iter().map(|m| m.content.clone()))
        .collect();
    assert!(all.iter().all(|text| !text.contains(SECRET_MARK)));
    assert!(
        texts(&env, Role::Scout)[0].contains(&format!("열쇠 {MASK} 쓰지 마라")),
        "가린 공지 글이 브리핑에 있어야 한다"
    );
}

// 검증: R17-2 V72 (판단의 자료 목록에 "사용자 공지" 한 줄이 가장 늦은 보낸 시각으로 든다)
#[tokio::test]
async fn 판단의_자료에_공지_한_줄이_가장_늦은_시각으로_든다() {
    let env = Env::new();
    let trade = full_script(&env, &leader_buy());
    let options = DiscussionOptions::default().with_notices(vec![
        notice("다음 주 실적 발표를 조심하라", 100),
        notice("손절 선은 꼭 지켜라", 5_000),
    ]);
    env.team()
        .discuss_with(request(trade), options)
        .await
        .unwrap();
    let sources = env.store.get_trade(trade).unwrap().sources;
    let known = UnixMillis(NOW.0 - 1_000);
    assert_eq!(sources.len(), 3);
    assert!(sources.contains(&source("공시", known)));
    assert!(sources.contains(&source("시세", known)));
    assert!(sources.contains(&source(NOTICE_SOURCE_NAME, UnixMillis(NOW.0 - 100))));
    assert_eq!(NOTICE_SOURCE_NAME, "사용자 공지");
}

// 검증: R17-2 E32 (공지는 자료 완전성에 들지 않는다: 자료가 비면 공지가 있어도 지켜본다)
#[tokio::test]
async fn 공지가_있어도_빈_자료는_지켜보기이고_담당을_부르지_않는다() {
    let env = Env::new();
    let mut req = request(env.trade());
    req.data[1].content = None;
    let options = DiscussionOptions::default().with_notices(vec![notice("공지다", 100)]);
    let out = env.team().discuss_with(req, options).await.unwrap();
    assert_eq!(
        out.decision,
        Decision::Watch(WatchReason::MissingData(vec!["시세".to_string()]))
    );
    assert!(env.ai.requests().is_empty());
}

// 검증: R17-2 V72 (공지를 안 넘기거나 빈 목록이면 지금과 같다: 공지 묶음도 한 줄도 없다)
#[tokio::test]
async fn 공지가_없거나_빈_목록이면_공지_묶음도_자료_한_줄도_없다() {
    for options in [
        DiscussionOptions::default(),
        DiscussionOptions::default().with_notices(Vec::new()),
    ] {
        let env = Env::new();
        let trade = full_script(&env, &leader_buy());
        env.team()
            .discuss_with(request(trade), options)
            .await
            .unwrap();
        assert!(!texts(&env, Role::Scout)[0].contains("사용자 공지"));
        let known = UnixMillis(NOW.0 - 1_000);
        assert_eq!(
            env.store.get_trade(trade).unwrap().sources,
            vec![source("공시", known), source("시세", known)]
        );
    }
}

/// 받은 신호를 모으는 포트.
#[derive(Default)]
struct Seen {
    all: Mutex<Vec<ProgressSignal>>,
}

impl ProgressSink for Seen {
    fn signal(&self, signal: ProgressSignal) -> Result<(), SignalError> {
        self.all.lock().unwrap().push(signal);
        Ok(())
    }
}

// 검증: R17-2 M9 (공지 옵션은 다른 옵션과 함께 쓰인다: 신호 포트도 그대로 일한다)
#[tokio::test]
async fn 공지_옵션은_신호_포트와_함께_쓰인다() {
    let env = Env::new();
    let trade = full_script(&env, &leader_summary());
    let sink = Arc::new(Seen::default());
    let options = DiscussionOptions::default()
        .with_signals(sink.clone())
        .with_notices(vec![notice("공지다", 100)]);
    let out = env
        .team()
        .discuss_with(request(trade), options)
        .await
        .unwrap();
    assert_eq!(out.decision, Decision::Buy);
    let all = sink.all.lock().unwrap().clone();
    assert!(matches!(all.first(), Some(ProgressSignal::Opened { .. })));
    assert_eq!(all.last(), Some(&ProgressSignal::Closed));
    assert!(texts(&env, Role::Scout)[0].contains("사용자 공지"));
}
