//! 시연이 아닌 조립: 모의계좌 · 주문 길 달력 없음(시세 흐름이 받은 달력만 쓴다) · 공식 팀(가짜 자료 서버) ·
//! 가짜 증권사 · 가짜 AI · 닿지 않는 텔레그램 자리. round42의 조립을 본떴다(그 폴더는 고치지 않는다).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64};
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_data::{CorpCode, DataConfig, NaverNewsConfig};
use pi_market::{Clock, Currency, SessionKind, StalePolicy, Symbol, TossConfig};
use pi_paper::{
    InitialCash, LimitFillPrice, MarketRules, MaxFill, MoneyRounding, PaperConfig, ProceedsUsable,
    RoundingMode, SessionRules, Settlement, TickBand,
};
use pi_rules::RulesConfig;
use pi_secrets::{
    Credentials, KeyKind, KeySlot, ProviderKeys, SecretStore, Verifier, VerifyFailure,
};
use price_inha::{
    App, Launch, MessengerWiring, Reach, Settings, TeamWiring, TradingWiring, Upstreams, Wiring,
};
use rust_decimal::Decimal;
use tempfile::TempDir;
use wiremock::MockServer;

use crate::common::STEP;
use crate::outside::{CORP_HYNIX, FakeAi, HYNIX, Holidays, SAMSUNG, TestClock};

fn d(text: &str) -> Decimal {
    text.parse().unwrap()
}

// ───────────── 조립 ─────────────

fn market_rules(currency: Currency, scale: u32, ticks: Vec<TickBand>) -> MarketRules {
    MarketRules {
        currency,
        commission_rate: d("0.00015"),
        sell_tax_rate: d("0.0020"),
        rounding: MoneyRounding {
            scale,
            mode: RoundingMode::Down,
        },
        tick_table: ticks,
        price_limit_rate: Some(d("0.30")),
        settlement: Settlement {
            business_days: 2,
            proceeds_usable: ProceedsUsable::AfterSettlement,
        },
        market_slippage_rate: d("0.001"),
        limit_fill_price: LimitFillPrice::Limit,
        max_fill: MaxFill::Unlimited,
        sessions: SessionRules {
            limit: vec![SessionKind::Regular],
            market: vec![SessionKind::Regular],
        },
        high_value_confirm_from: None,
        max_order_amount: None,
    }
}

/// 주문 길 조립: 모의계좌. **달력은 주지 않는다** — 시세 흐름이 가짜 증권사에서 받은 달력(오늘 것)만 쓴다.
fn trading_wiring() -> TradingWiring {
    let tick = |below: Option<&str>, tick: &str| TickBand {
        below: below.map(d),
        tick: d(tick),
    };
    let paper = PaperConfig {
        initial_cash: InitialCash {
            krw: d("10000000"),
            usd: d("10000"),
        },
        kr: market_rules(
            Currency::Krw,
            0,
            vec![
                tick(Some("50000"), "50"),
                tick(Some("200000"), "100"),
                tick(None, "1000"),
            ],
        ),
        us: market_rules(Currency::Usd, 2, vec![tick(None, "0.01")]),
        stale: StalePolicy {
            max_age: Duration::from_secs(30),
        },
        event_buffer: 64,
    };
    let rules = RulesConfig {
        sectors: HashMap::from([(Symbol::new(HYNIX), "반도체".to_string())]),
        ..RulesConfig::default()
    };
    TradingWiring {
        paper,
        rules,
        calendar: None,
    }
}

fn team_wiring(data: &MockServer) -> TeamWiring {
    let mut team = TeamWiring::official();
    let uri = data.uri();
    team.data = DataConfig::new(uri.clone(), uri.clone(), uri.clone());
    team.data.timeout = Duration::from_secs(3);
    team.corp_codes = HashMap::from([(Symbol::new(HYNIX), CorpCode::parse(CORP_HYNIX).unwrap())]);
    team.news = NaverNewsConfig::new(uri).with_timeout(Duration::from_secs(3));
    team.data_poll = Duration::from_secs(3600);
    team
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn settings(home: &Path) -> Settings {
    let symbols = format!("{HYNIX},{SAMSUNG}");
    let args: Vec<String> = [
        "--home",
        home.to_str().unwrap(),
        "--port",
        "0",
        "--interval-ms",
        "20",
        "--retry-initial-ms",
        "20",
        "--retry-max-ms",
        "40",
        "--symbols",
        &symbols,
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    Settings::parse(&args, &|_| None, None).expect("설정을 읽어야 한다")
}

/// 무엇이든 받는 시험용 확인자(바깥으로 나가지 않는다).
struct AcceptAll;

impl Verifier for AcceptAll {
    async fn verify(&self, _kind: KeyKind, _values: &Credentials) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_slot(
        &self,
        _slot: KeySlot,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }

    async fn verify_provider(
        &self,
        _keys: ProviderKeys,
        _values: &Credentials,
    ) -> Result<(), VerifyFailure> {
        Ok(())
    }
}

/// 필수 키(증권사 · AI)와 자료 키를 시험용 값으로 등록한 새 보관 폴더. 값은 진짜 키처럼 보이지 않는다.
pub async fn fresh_home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let secrets = SecretStore::open(dir.path()).unwrap();
    let broker = Credentials::new()
        .with("client_id", "zz-round55-client-id-0001")
        .with("client_secret", "zz-round55-client-secret-0002");
    secrets
        .register(KeyKind::Broker, broker, &AcceptAll)
        .await
        .unwrap();
    let ai = Credentials::new().with("api_key", "zz-round55-ai-key-qwmv-0003");
    secrets
        .register(KeyKind::AiApiKey, ai, &AcceptAll)
        .await
        .unwrap();
    for (slot, value) in [
        (KeySlot::Dart, "zz-round55-dart-key-0004"),
        (KeySlot::Ecos, "zz-round55-ecos-key-0005"),
    ] {
        let values = Credentials::new().with(slot.fields()[0], value);
        secrets
            .register_slot(slot, values, &AcceptAll)
            .await
            .unwrap();
    }
    dir
}

/// 다시 띄워도 그대로 쓰는 바깥 자리.
pub struct Parts {
    pub clock: Arc<TestClock>,
    pub price: Arc<AtomicI64>,
    pub muted: Arc<AtomicBool>,
    pub holidays: Holidays,
    pub broker: MockServer,
    pub ai: FakeAi,
    pub data: MockServer,
}

pub async fn launch(home: &Path, parts: &Parts) -> App {
    let launch = Launch {
        trading: trading_wiring(),
        team: Some(team_wiring(&parts.data)),
        secrets: None,
    };
    let wiring = Wiring {
        upstreams: Upstreams {
            toss: TossConfig {
                base_url: parts.broker.uri(),
                timeout: Duration::from_secs(3),
                token_failure_cooldown: Duration::ZERO,
                ..TossConfig::default()
            },
            anthropic: AnthropicConfig {
                base_url: parts.ai.uri(),
                timeout: Duration::from_secs(3),
                max_retries: 0,
                retry_base_delay: Duration::from_millis(1),
            },
        },
        clock: Arc::clone(&parts.clock) as Arc<dyn Clock>,
        peer_override: None,
        cli_locations: Vec::new(),
    };
    let reach = Reach {
        push: None,
        backup: None,
    };
    let messenger = MessengerWiring {
        telegram_api_base: dead_base_url(),
        ..MessengerWiring::default()
    };
    let launched = App::launch_with_messenger(settings(home), wiring, launch, reach, messenger);
    tokio::time::timeout(STEP, launched)
        .await
        .expect("서버가 시간 안에 떠야 한다")
        .expect("서버가 떠야 한다")
}
