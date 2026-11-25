//! 40차 1번: 사람이 내리는 전체 정지 `POST /api/team/stop`(결정 25 · F4 · R14-1 · 결정 20) — 운용 중 · 쉬는 중에서
//! 전체 정지(기록 사유 `OwnerStop` · 내 승인 표시) · 이미 정지면 409 · 세션 없으면 401 · 원격 세션으로도 됨 · 정지 뒤
//! 새 사는 주문은 막히고 다시 시작은 지금 규칙대로 · 담당(AI) 도구에는 정지가 없음.

use std::sync::Arc;
use std::time::Duration;

use pi_agent::Role;
use pi_market::{Clock, Symbol, TossConfig};
use pi_store::{TeamCause, TeamState, TeamStateRecord};
use price_inha::{
    App, DemoTurn, Placed, STOP_ALREADY_STOPPED, Settings, TEAM_FULL_STOP, TEAM_PATH,
    TEAM_RESTART_PATH, TEAM_RESTING, TEAM_STOP_PATH, Upstreams, run_assembly, save_watchlist,
};
use reqwest::Method;
use serde_json::json;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    Bench, FakeTelegram, HYNIX, QUIET, Reply, STEP, fresh_home, get, login, post, post_empty,
    send_anonymous, send_with,
};

/// 마지막 팀 상태 기록.
fn last_record(bench: &Bench) -> TeamStateRecord {
    bench
        .store()
        .team_history()
        .unwrap()
        .last()
        .cloned()
        .expect("기록이 하나는 있다")
}

/// 정지가 남긴 기록인가: 전체 정지로 · 사유 `OwnerStop` · 내 승인 표시.
fn assert_owner_stop(record: &TeamStateRecord, from: TeamState) {
    assert_eq!(record.from, from, "{record:?}");
    assert_eq!(record.to, TeamState::FullStop, "{record:?}");
    assert_eq!(record.cause, TeamCause::OwnerStop, "{record:?}");
    assert!(record.owner_approved, "내 승인 표시: {record:?}");
}

/// 성공 응답: 다시 시작과 같은 모양(바뀐 팀 상태 글자).
fn assert_stopped_reply(reply: &Reply) {
    assert_eq!(reply.status, 200, "{}", reply.text);
    assert_eq!(reply.body["state"], json!(TEAM_FULL_STOP), "{}", reply.text);
}

// 검증: V9 F4 R14-1 M1 C2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 운용_중에_정지를_누르면_전체_정지가_되고_사유는_내가_정지다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    bench.ready().await;
    let before = bench.store().team_history().unwrap().len();

    let reply = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_stopped_reply(&reply);
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    let history = bench.store().team_history().unwrap();
    assert_eq!(history.len(), before + 1, "기록 한 줄이 더해진다");
    assert_owner_stop(&last_record(&bench), TeamState::Operating);

    let team = get(bench.addr(), TEAM_PATH).await;
    assert_eq!(team.body["state"], json!(TEAM_FULL_STOP), "{}", team.text);
    bench.close().await;
}

// 검증: V9 F4 R14-1 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 쉬는_중에도_빈_몸통_객체로_정지하면_전체_정지가_된다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::Resting);

    let reply = post(bench.addr(), TEAM_STOP_PATH, &json!({})).await;
    assert_stopped_reply(&reply);
    assert_owner_stop(&last_record(&bench), TeamState::Resting);
    bench.close().await;
}

// 검증: V9 E4 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 이미_전체_정지면_409이고_기록을_더하지_않는다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let first = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_stopped_reply(&first);
    let count = bench.store().team_history().unwrap().len();

    let again = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_eq!(again.status, 409, "{}", again.text);
    assert_eq!(
        again.body["error"],
        json!(STOP_ALREADY_STOPPED),
        "{}",
        again.text
    );
    assert_eq!(bench.store().team_history().unwrap().len(), count);
    assert_eq!(bench.store().team_state().unwrap(), TeamState::FullStop);
    bench.close().await;
}

// 검증: C2 C15 D2 E3
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 세션이_없으면_401이고_팀은_그대로다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let count = bench.store().team_history().unwrap().len();

    let reply = send_anonymous(bench.addr(), Method::POST, TEAM_STOP_PATH, None).await;
    assert_eq!(reply.status, 401, "{}", reply.text);
    bench.go_remote();
    let reply = send_anonymous(bench.addr(), Method::POST, TEAM_STOP_PATH, None).await;
    assert_eq!(reply.status, 401, "{}", reply.text);

    assert_eq!(bench.store().team_state().unwrap(), TeamState::Resting);
    assert_eq!(bench.store().team_history().unwrap().len(), count);
    bench.close().await;
}

// 검증: V9 R14-1 C15 M1
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 다른_기기의_세션으로도_정지할_수_있다() {
    // 정지는 위험을 줄이는 쪽이라 이 컴퓨터로 막지 않는다(결정 25).
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    bench.ready().await;
    let cookie = bench.remote_login().await;

    let reply = send_with(bench.addr(), Method::POST, TEAM_STOP_PATH, &cookie, None).await;
    assert_stopped_reply(&reply);
    assert_owner_stop(&last_record(&bench), TeamState::Operating);
    bench.close().await;
}

// 검증: F4 V9 R14-1 V85 E9
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 정지_뒤에는_새_사는_주문이_막히고_다시_시작은_지금_규칙대로_된다() {
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    bench.ready().await;
    // 정지 전에는 같은 주문 안이 승인을 기다린다(발판).
    let (_number, _fingerprint) = bench.awaiting().await;

    let reply = post_empty(bench.addr(), TEAM_STOP_PATH).await;
    assert_stopped_reply(&reply);

    let placed = bench.place_buy().await;
    assert!(
        !matches!(
            placed,
            Ok(Placed::Ordered(_) | Placed::AwaitingApproval { .. })
        ),
        "전체 정지 뒤 새 사는 주문은 나가지도 승인을 기다리지도 않는다: {placed:?}"
    );

    // 다시 시작은 지금 규칙(결정 20): 세션이 있는 요청이면 쉬는 중으로, 기록 사유는 내가 다시 시작.
    let restart = post_empty(bench.addr(), TEAM_RESTART_PATH).await;
    assert_eq!(restart.status, 200, "{}", restart.text);
    assert_eq!(
        restart.body["state"],
        json!(TEAM_RESTING),
        "{}",
        restart.text
    );
    let record = last_record(&bench);
    assert_eq!(record.cause, TeamCause::OwnerRestart, "{record:?}");
    assert_eq!(record.to, TeamState::Resting, "{record:?}");
    bench.close().await;
}

// 검증: V9 R14-1 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 정지_경로는_세션이_있는_같은_출처의_요청만_받는다() {
    // 다른 출처(`Origin`)의 쓰기는 관문이 막는다 — 정지도 다른 쓰기와 같다.
    let tg = FakeTelegram::start().await;
    let bench = Bench::start(&tg).await;
    let cookie = login(bench.addr()).await;
    let reply = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}{TEAM_STOP_PATH}",
            bench.addr().port()
        ))
        .header("Origin", "http://evil.invalid")
        .header("Cookie", &cookie)
        .send()
        .await
        .expect("요청이 닿아야 한다");
    assert!(
        reply.status().is_client_error(),
        "다른 출처는 받지 않는다: {}",
        reply.status()
    );
    tokio::time::sleep(QUIET).await;
    assert_eq!(bench.store().team_state().unwrap(), TeamState::Resting);
    bench.close().await;
}

// ───────────── 담당 도구 ─────────────

/// 무엇을 물어도 503으로 답하는 로컬 가짜 서버(시연 조립의 바깥 자리).
async fn dead_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

/// 시연 조립(가짜 AI · 가짜 시세 · 모의계좌)으로 팀을 이어 띄운다. 바깥 자리는 모두 로컬 가짜 서버다.
async fn demo_app(home: &std::path::Path, outside: &MockServer) -> App {
    save_watchlist(home, &[Symbol::new(HYNIX)]).unwrap();
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--demo",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    let settings = Settings::parse(&args, &|_| None, None).expect("설정을 읽어야 한다");
    let mut assembly = run_assembly(&settings, None);
    assembly.wiring.clock = Arc::new(pi_market::SystemClock) as Arc<dyn Clock>;
    assembly.wiring.cli_locations = Vec::new();
    assembly.wiring.upstreams = Upstreams {
        toss: TossConfig {
            base_url: outside.uri(),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: pi_ai::AnthropicConfig {
            base_url: outside.uri(),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    };
    if let Some(team) = assembly.launch.team.as_mut() {
        let uri = outside.uri();
        team.data = pi_data::DataConfig::new(uri.clone(), uri.clone(), uri.clone());
        team.data.timeout = Duration::from_secs(3);
        team.news = pi_data::NaverNewsConfig::new(uri).with_timeout(Duration::from_secs(3));
    }
    assembly.demo_ai_pause = Duration::ZERO;
    tokio::time::timeout(STEP, App::launch_run(settings, assembly))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}

// 검증: V9 R14-1 V22
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 담당은_정지를_도구로_부를_수_없다() {
    // 담당(AI)에게 정지 길을 주지 않는다(결정 25). 정지를 뜻하는 이름의 도구를 불러도 "그런 도구가 없습니다"이고
    // 팀은 전체 정지가 되지 않는다.
    let home = fresh_home().await;
    let outside = dead_server().await;
    let app = demo_app(home.path(), &outside).await;
    let crew = app.crew().expect("시연 조립은 팀을 잇는다");
    let ai = crew.demo_ai().expect("시연 모드다");
    for name in ["team_stop", "stop"] {
        let call = format!(r#"{{"action": "{name}", "args": {{}}}}"#);
        ai.script(DemoTurn::Talk(Role::Scout), call);
        let _ = tokio::time::timeout(STEP, crew.discuss(HYNIX, "정지 도구 시험")).await;
        let unknown = format!("그런 도구가 없습니다: {name}");
        assert!(
            ai.prompts().iter().any(|p| p.contains(&unknown)),
            "{name}은 담당 도구가 아니다"
        );
        assert_ne!(
            app.store().team_state().unwrap(),
            TeamState::FullStop,
            "담당이 팀을 멈추지 못한다"
        );
    }
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}
