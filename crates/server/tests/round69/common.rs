//! 69차 시험의 공용 발판: 서버 띄우기(시연 · 임시 보관 폴더 · 빈 포트), 그 컴퓨터 세션, HTTP 읽기,
//! 논의중 · 판단된 매매 만들기, 매매 방(`trade-<번호>`)에 방 로그 줄 넣기. round56 · round31 발판을 본떴다.
//!
//! 팀은 잇되(근거 경로가 열리게) 자료 주소 · AI 주소 · 토스 주소는 아무도 듣지 않는 로컬 주소이고, 종목 고유번호가
//! 없어 팀이 스스로 논의를 열지 않는다. 방 로그 줄은 실행기(`pi_agent`)가 쓰는 모양 그대로 테스트가 넣는다.

use std::net::SocketAddr;
use std::time::Duration;

use pi_ai::AnthropicConfig;
use pi_data::DataConfig;
use pi_market::TossConfig;
use pi_store::{
    AccountKind, Amount, Judgment, NewRoomEvent, NewTrade, RoleScore, RoomActorKind, SourceRef,
    Store, TradeId, TradeSide, UnixMillis,
};
use price_inha::{App, Launch, Reach, Settings, TeamWiring, TradingWiring, Upstreams, Wiring};
use serde_json::{Value, json};
use tempfile::TempDir;

/// 한 걸음의 기다림 상한.
pub const STEP: Duration = Duration::from_secs(15);

/// 매매하는 종목(NAVER — 체험판에서 지켜보기로 끝난 매매의 종목).
pub const NAVER: &str = "035420";

fn no_env(_name: &str) -> Option<String> {
    None
}

/// 설정: 그 보관 폴더 · 빈 포트 · 시연.
fn settings(home: &std::path::Path) -> Settings {
    let args: Vec<String> = ["--home", home.to_str().unwrap(), "--port", "0", "--demo"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    Settings::parse(&args, &no_env, None).expect("설정을 읽어야 한다")
}

/// 아무도 듣지 않는 로컬 주소(실제 바깥 주소로 나가지 않게).
fn dead_base_url() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

/// 바깥 자료 자리도 아무도 듣지 않는 주소이고, 종목 고유번호가 없는 팀.
fn quiet_team(dead: &str) -> TeamWiring {
    let mut team = TeamWiring::official();
    team.data = DataConfig::new(dead.to_string(), dead.to_string(), dead.to_string());
    team.data.timeout = Duration::from_secs(1);
    team.corp_codes.clear();
    team
}

/// 떠 있는 서버 하나와 그 보관 폴더.
pub struct Bench {
    _dir: TempDir,
    app: App,
}

impl Bench {
    /// 새 보관 폴더로 띄운다.
    pub async fn start() -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let dead = dead_base_url();
        let wiring = Wiring {
            upstreams: Upstreams {
                toss: TossConfig {
                    base_url: dead.clone(),
                    timeout: Duration::from_secs(1),
                    token_failure_cooldown: Duration::ZERO,
                    ..TossConfig::default()
                },
                anthropic: AnthropicConfig {
                    base_url: dead.clone(),
                    timeout: Duration::from_secs(1),
                    max_retries: 0,
                    retry_base_delay: Duration::from_millis(1),
                },
            },
            cli_locations: Vec::new(),
            ..Wiring::default()
        };
        let launch = Launch {
            trading: TradingWiring::default(),
            team: Some(quiet_team(&dead)),
            secrets: None,
        };
        let reach = Reach {
            push: None,
            backup: None,
        };
        let start = App::launch_with(settings(dir.path()), wiring, launch, reach);
        let app = tokio::time::timeout(STEP, start)
            .await
            .expect("서버가 시간 안에 떠야 한다")
            .expect("서버가 떠야 한다");
        Bench { _dir: dir, app }
    }

    pub fn addr(&self) -> SocketAddr {
        self.app.local_addr()
    }

    pub fn store(&self) -> Store {
        self.app.store()
    }

    /// 논의중 매매 하나(만든 시각 `at`).
    pub fn discussing(&self, at: i64) -> TradeId {
        self.store()
            .create_trade(NewTrade {
                symbol: NAVER.to_string(),
                account: AccountKind::Paper,
                at: UnixMillis(at),
            })
            .unwrap()
            .id
    }

    /// 논의중 매매를 판단됨으로(근거 6칸 · 담당 점수 하나, 방향 사기).
    pub fn judge(&self, trade: TradeId, at: i64) {
        let judgment = Judgment {
            decided_at: UnixMillis(at),
            rationale: pi_store::Rationale {
                hypothesis: Some("실적이 좋아진다".to_string()),
                trigger: Some("공시".to_string()),
                entry: Some(Amount(100_000)),
                stop: Some(Amount(95_000)),
                target: Some(Amount(110_000)),
                expected_value: Some(Amount(5_000)),
            },
            sources: vec![SourceRef {
                name: "공시".to_string(),
                known_at: UnixMillis(at),
            }],
            scores: vec![RoleScore {
                role: "analyst".to_string(),
                score: 70,
                reason: "좋다".to_string(),
            }],
            consensus_score: 70,
        };
        self.store()
            .judge_trade_with_side(trade, judgment, TradeSide::Buy)
            .unwrap();
    }

    /// 그 매매의 방(`trade-<번호>`)에 줄 하나를 넣는다.
    fn room_line(
        &self,
        trade: TradeId,
        line: (&str, RoomActorKind, &str),
        payload: Value,
        at: i64,
    ) {
        let (kind, actor_kind, actor_id) = line;
        let room = format!("trade-{}", trade.0);
        let seq = self.store().room_last_seq(&room).unwrap();
        self.store()
            .append_room_event(NewRoomEvent {
                room,
                event_id: format!("test-{}-{seq}", trade.0),
                kind: kind.to_string(),
                actor_kind,
                actor_id: actor_id.to_string(),
                payload,
                at: UnixMillis(at),
            })
            .unwrap();
    }

    /// 담당의 글(`message.member`) — 새 논의가 열려 담당이 말했다.
    pub fn member_said(&self, trade: TradeId, at: i64) {
        let line = ("message.member", RoomActorKind::Member, "analyst");
        self.room_line(trade, line, json!({"text": "자료를 봅니다"}), at);
    }

    /// 실행기가 남기는 지켜보기 끝(`room.activity {"status": "watch", "reason_code"}`).
    pub fn watched(&self, trade: TradeId, code: &str, at: i64) {
        let line = ("room.activity", RoomActorKind::Gateway, "agent");
        let payload = json!({"status": "watch", "reason_code": code});
        self.room_line(trade, line, payload, at);
    }

    /// 서버가 남기는 논의 오류 끝(`room.activity {"status": "failed", "reason_code"}`).
    pub fn failed(&self, trade: TradeId, code: &str, at: i64) {
        let line = ("room.activity", RoomActorKind::Gateway, "server");
        let payload = json!({"status": "failed", "reason_code": code});
        self.room_line(trade, line, payload, at);
    }

    /// 논의 뒤 서버가 남기는 다른 줄(버린 뉴스 기사 수) — 새 논의가 아니다.
    pub fn server_note(&self, trade: TradeId, at: i64) {
        let line = ("room.activity", RoomActorKind::Gateway, "server");
        let payload =
            json!({"news_dropped": 1, "text": "링크가 이상한 뉴스 기사 1건을 버렸습니다"});
        self.room_line(trade, line, payload, at);
    }

    pub async fn close(self) {
        tokio::time::timeout(STEP, self.app.shutdown())
            .await
            .expect("시간 안에 꺼져야 한다")
            .expect("끄기가 성공해야 한다");
    }
}

// ───────────── HTTP ─────────────

fn origin_of(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

/// 그 컴퓨터 세션의 `Cookie` 머리글 글자.
async fn login(addr: SocketAddr) -> String {
    let origin = origin_of(addr);
    let request = reqwest::Client::new()
        .post(format!("{origin}{}", pi_gate::SESSION_PATH))
        .header("Origin", &origin);
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("세션 요청이 닿아야 한다");
    assert!(reply.status().is_success(), "세션을 받아야 한다");
    let raw = reply.headers().get("set-cookie").unwrap().to_str().unwrap();
    raw.split(';').next().unwrap().trim().to_string()
}

/// 세션을 받아 읽는다. 상태와 JSON 본문.
pub async fn get(addr: SocketAddr, path: &str) -> (u16, Value) {
    let cookie = login(addr).await;
    let request = reqwest::Client::new()
        .get(format!("{}{path}", origin_of(addr)))
        .header("Cookie", cookie);
    let reply = tokio::time::timeout(STEP, request.send())
        .await
        .expect("응답이 시간 안에 와야 한다")
        .expect("요청이 닿아야 한다");
    let status = reply.status().as_u16();
    let body = reply.json::<Value>().await.unwrap_or(Value::Null);
    (status, body)
}
