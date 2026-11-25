//! 39차 10번: 시연 모드는 메신저 루프를 돌리지 않는다 — 토큰이 있어도 텔레그램을 부르지 않고, 등록하려 하면 시연 안내
//! (실행 파일 조립 `run_assembly --demo`, 바깥 자리는 모두 받은 요청을 세는 로컬 가짜 서버). V87 · E56.

use std::sync::Arc;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_data::DataConfig;
use pi_market::{Clock, Symbol, TossConfig};
use price_inha::{
    App, MESSENGER_DEMO_REFUSED, MESSENGER_PATH, Settings, TELEGRAM_KEYS, Upstreams, run_assembly,
    save_watchlist,
};
use serde_json::json;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::common::{
    Bench, FakeTelegram, HYNIX, QUIET, STEP, TOKEN, fresh_home, get, messenger_wiring, post,
    register_provider,
};

/// 무엇을 물어도 503으로 답하고 받은 요청을 세는 로컬 가짜 서버.
async fn counting_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    server
}

async fn received(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .map_or(0, |list| list.len())
}

// 검증: V87 E56 D2
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연_모드는_메신저를_돌리지_않고_가짜_서버_밖으로_나가지_않는다() {
    let tg = FakeTelegram::start().await;
    let home = fresh_home().await;
    register_provider(home.path(), TELEGRAM_KEYS, "bot_token", TOKEN).await;
    save_watchlist(home.path(), &[Symbol::new(HYNIX)]).unwrap();
    let (broker, ai, data, news) = (
        counting_server().await,
        counting_server().await,
        counting_server().await,
        counting_server().await,
    );

    let args: Vec<String> = [
        "--home",
        home.path().to_str().unwrap(),
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
    // 시계 · 바깥 주소 · CLI 찾는 자리 · 가짜 AI 뜸 · 메신저 주소만 바꾼다(round37 Demo와 같은 길).
    let clock = Arc::new(pi_market::SystemClock);
    assembly.wiring.clock = clock as Arc<dyn Clock>;
    assembly.wiring.cli_locations = Vec::new();
    assembly.wiring.upstreams = Upstreams {
        toss: TossConfig {
            base_url: broker.uri(),
            timeout: Duration::from_secs(3),
            token_failure_cooldown: Duration::ZERO,
            ..TossConfig::default()
        },
        anthropic: AnthropicConfig {
            base_url: ai.uri(),
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
        },
    };
    if let Some(team) = assembly.launch.team.as_mut() {
        let data_uri = data.uri();
        team.data = DataConfig::new(data_uri.clone(), data_uri.clone(), data_uri);
        team.data.timeout = Duration::from_secs(3);
        team.news = pi_data::NaverNewsConfig::new(news.uri()).with_timeout(Duration::from_secs(3));
    }
    assembly.demo_ai_pause = Duration::ZERO;
    assembly.messenger = messenger_wiring(&tg);
    let app = tokio::time::timeout(STEP, App::launch_run(settings, assembly))
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다");
    let addr = app.local_addr();

    tokio::time::sleep(QUIET).await;
    let status = get(addr, MESSENGER_PATH).await;
    assert_eq!(status.status, 200, "{}", status.text);
    assert_eq!(
        status.body["telegram"]["state"],
        json!("off"),
        "{}",
        status.text
    );

    let reply = post(
        addr,
        price_inha::MESSENGER_TELEGRAM_PATH,
        &json!({ "token": TOKEN }),
    )
    .await;
    assert_eq!(reply.status, 409, "{}", reply.text);
    assert_eq!(reply.body["error"], json!("demo_mode"), "{}", reply.text);
    assert_eq!(
        reply.body["message"],
        json!(MESSENGER_DEMO_REFUSED),
        "{}",
        reply.text
    );

    tokio::time::sleep(QUIET).await;
    assert!(tg.requests().await.is_empty(), "텔레그램을 부르지 않는다");
    for (name, server) in [
        ("증권사", &broker),
        ("AI", &ai),
        ("자료", &data),
        ("네이버", &news),
    ] {
        assert_eq!(received(server).await, 0, "{name}로 나가지 않는다");
    }
    tokio::time::timeout(STEP, app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

// 검증: V87
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn 시연이_아니면_띄울_때_저장된_토큰으로_받기를_시작한다() {
    let tg = FakeTelegram::start().await;
    let home = fresh_home().await;
    register_provider(home.path(), TELEGRAM_KEYS, "bot_token", TOKEN).await;
    let bench = Bench::start_in(home, &tg).await;
    bench.wait_state("polling").await;
    tg.wait_count("getUpdates", 1).await;
    let status = bench.status().await;
    assert_eq!(status["telegram"]["configured"], json!(true), "{status}");
    bench.close().await;
}
