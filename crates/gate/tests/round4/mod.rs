//! 4차 작업: 원본 대조의 "모르고 다름"(D17 살아 있음 신호 · D21 보안 응답 헤더 · C9 이벤트 종류 이름)과
//! store 3차 검토의 참고 8(시각 칸 · 저장된 이름).
//!
//! 근거: `docs/dev/modules/gate.md` "4차 작업", `docs/dev/conformance.md` 4장 D17 · D21 · C9.
//! 원본에서 직접 확인한 것(Hermes, 읽기만 했다):
//! - D21: `gateway/platforms/api_server.py:633-640`의 `_SECURITY_HEADERS`
//!   (`X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` 등)와
//!   `:681-687`의 미들웨어(모든 응답 · 오류 응답에 `setdefault`로 붙인다 — 처리기가 붙인 것은 그대로다).
//! - D17: `gateway/platforms/api_server_runs.py:1325-1331`(큐가 `CHAT_COMPLETIONS_SSE_KEEPALIVE_SECONDS`
//!   동안 조용하면 `: keepalive`를 쓴다)와 `api_server.py:173`(`= 10.0`초).
//! - C9: `gateway/hosted_rooms.py:27`(`MAX_EVENT_KIND_CHARS = 64`), `:48`(`^[a-z][a-z0-9_.-]*$`).
//!
//! 테스트 쪽 WebSocket 클라이언트(tungstenite 0.30)는 ping을 읽으면 pong을 줄에 넣고, 다음에 읽거나 쓸 때
//! 보낸다(`tungstenite-0.30.0/src/protocol/mod.rs:284-286`, `:447`). 그래서 "pong이 없는 클라이언트"는
//! 정한 시간 동안 **읽지 않는** 클라이언트로 흉내 낸다.
//!
//! 시간에 기대는 테스트는 간격을 작게 설정해 실제 시간으로 재고 범위에 여유를 둔다.

use super::*;
use pi_gate::{
    DEFAULT_KEEPALIVE_INTERVAL, DEFAULT_PONG_TIMEOUT, SECURITY_HEADERS, valid_event_kind,
};
use pi_store::{EVENT_KIND_MAX_CHARS, EventRange, StoreError, TimedEvent};

// ───────────── 도우미 ─────────────

/// 그 이름의 헤더 값들(대소문자를 가리지 않는다).
fn header_values<'a>(reply: &'a Reply, name: &str) -> Vec<&'a str> {
    reply
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .collect()
}

/// 원본과 같은 보안 헤더가 한 번씩, 원본의 값으로 붙어 있다.
fn assert_security_headers(reply: &Reply, what: &str) {
    for (name, value) in [
        ("X-Content-Type-Options", "nosniff"),
        ("X-Frame-Options", "DENY"),
        ("Referrer-Policy", "no-referrer"),
    ] {
        assert_eq!(
            header_values(reply, name),
            [value],
            "{what}(HTTP {}): {name}가 원본 값으로 한 번 붙어야 한다",
            reply.status
        );
    }
}

/// `wait` 동안 읽으며(그래서 ping에 pong이 나간다) 온 ping 수와 연결이 닫혔는지를 센다.
async fn pings_while_reading(ws: &mut Ws, wait: Duration) -> (usize, bool) {
    let deadline = tokio::time::Instant::now() + wait;
    let mut pings = 0;
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => return (pings, false),
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return (pings, true),
            Ok(Some(Ok(Message::Ping(_)))) => pings += 1,
            Ok(Some(Ok(_))) => {}
        }
    }
}

/// 연결이 `wait` 안에 닫히는가(그동안 온 프레임은 버린다).
async fn closes_within(ws: &mut Ws, wait: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        match tokio::time::timeout_at(deadline, ws.next()).await {
            Err(_) => return false,
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => return true,
            Ok(Some(Ok(_))) => {}
        }
    }
}

/// 저장소 파일 버전 2 픽스처(`pi-store`의 테스트 자료, 읽기만 한다).
const STORE_V2_FIXTURE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../store/tests/fixtures/v2.db");

/// 픽스처의 시각 기준(`pi-store` 테스트의 `t(n)`).
const V2_T0: i64 = 1_800_000_000_000;

// ───────────── D21 보안 응답 헤더 ─────────────

// 검증: D2, C15, D11
#[tokio::test]
async fn 모든_응답에_원본과_같은_보안_헤더가_붙는다() {
    // 원본이 붙이는 헤더 가운데 지시서가 정한 셋이 공개 상수로 같은 이름 · 값이다.
    let expected = [
        ("X-Content-Type-Options", "nosniff"),
        ("X-Frame-Options", "DENY"),
        ("Referrer-Policy", "no-referrer"),
    ];
    assert_eq!(SECURITY_HEADERS, expected);

    let h = start().await;
    let (host, origin) = (host_of(h.addr), origin_of(h.addr));
    let cookie = login(h.addr).await;

    let session = enter(h.addr).await;
    assert_eq!(session.status, 204);
    assert_security_headers(&session, "세션 만들기");

    let public = http(h.addr, "GET", PUBLIC_PATH, &[("Host", &host)]).await;
    assert_eq!(public.status, 200);
    assert_security_headers(&public, "공개 경로");

    let protected = http(
        h.addr,
        "GET",
        PING_PATH,
        &[("Host", &host), ("Cookie", &cookie)],
    )
    .await;
    assert_eq!(protected.status, 200);
    assert_security_headers(&protected, "보호된 경로");

    let no_session = http(h.addr, "GET", PING_PATH, &[("Host", &host)]).await;
    assert_eq!(no_session.status, 401);
    assert_security_headers(&no_session, "세션 없음(401)");

    let other_origin = http(
        h.addr,
        "POST",
        POKE_PATH,
        &[
            ("Host", &host),
            ("Cookie", &cookie),
            ("Origin", "http://localhost:1"),
        ],
    )
    .await;
    assert!(other_origin.is_rejected(), "{}", other_origin.status);
    assert_security_headers(&other_origin, "다른 출처(4xx)");

    let bad_host = http(h.addr, "GET", PUBLIC_PATH, &[("Host", "rebind.invalid:80")]).await;
    assert!(bad_host.is_rejected(), "{}", bad_host.status);
    assert_security_headers(&bad_host, "허용하지 않는 Host");

    let missing = http(
        h.addr,
        "GET",
        "/no/such/path",
        &[("Host", &host), ("Cookie", &cookie)],
    )
    .await;
    assert!(missing.is_rejected(), "{}", missing.status);
    assert_security_headers(&missing, "없는 경로");

    let ws_refused = http(
        h.addr,
        "GET",
        WS_PATH,
        &[("Host", &host), ("Origin", &origin)],
    )
    .await;
    assert_eq!(ws_refused.status, 401);
    assert_security_headers(&ws_refused, "세션 없는 실시간 연결");
}

// 검증: C15
#[tokio::test]
async fn 그_컴퓨터_밖에서_온_요청을_거부할_때도_보안_헤더가_붙는다() {
    let h = start_with(|c| c, outside).await;
    let host = host_of(h.addr);
    let refused = http(h.addr, "GET", PUBLIC_PATH, &[("Host", &host)]).await;
    assert_eq!(refused.status, 401);
    assert_security_headers(&refused, "루프백이 아닌 상대(401)");
    let refused = enter(h.addr).await;
    assert_eq!(refused.status, 401);
    assert_security_headers(&refused, "루프백이 아닌 상대의 세션 요청");
}

// 검증: D11
#[tokio::test]
async fn 처리기가_정한_보안_헤더는_덮어쓰지_않는다() {
    // 원본은 `setdefault`로 붙인다: 처리기가 이미 붙인 같은 이름의 헤더는 그대로 둔다.
    let own = Router::new().route(
        "/own",
        get(|| async {
            (
                [
                    ("content-security-policy", "default-src 'self'"),
                    ("x-frame-options", "SAMEORIGIN"),
                ],
                "own",
            )
        }),
    );
    let h = start_with(|c| c, |b| b.public_routes(own)).await;
    let host = host_of(h.addr);
    let reply = http(h.addr, "GET", "/own", &[("Host", &host)]).await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        header_values(&reply, "content-security-policy"),
        ["default-src 'self'"],
        "화면이 정한 CSP는 그대로 한 번이다"
    );
    assert_eq!(
        header_values(&reply, "x-frame-options"),
        ["SAMEORIGIN"],
        "처리기가 정한 값을 덮어쓰거나 겹쳐 붙이지 않는다"
    );
    assert_eq!(header_values(&reply, "x-content-type-options"), ["nosniff"]);
    assert_eq!(header_values(&reply, "referrer-policy"), ["no-referrer"]);
}

// ───────────── D17 살아 있음 신호 ─────────────

// 검증: E3, D11
#[test]
fn 살아_있음_신호의_기본_간격은_원본처럼_10초다() {
    assert_eq!(DEFAULT_KEEPALIVE_INTERVAL, Duration::from_secs(10));
    let config = GateConfig::default();
    assert_eq!(config.keepalive_interval, DEFAULT_KEEPALIVE_INTERVAL);
    assert_eq!(config.pong_timeout, DEFAULT_PONG_TIMEOUT);
    assert!(
        DEFAULT_PONG_TIMEOUT > Duration::ZERO,
        "pong을 기다리는 시간이 있다"
    );
    let local = GateConfig::local(0);
    assert_eq!(local.keepalive_interval, DEFAULT_KEEPALIVE_INTERVAL);
    assert_eq!(local.pong_timeout, DEFAULT_PONG_TIMEOUT);
}

// 검증: E3, C2
#[tokio::test]
async fn 조용한_연결에도_정한_간격마다_ping이_오고_pong을_하면_이어진다() {
    let h = start_with(
        |c| GateConfig {
            keepalive_interval: Duration::from_millis(150),
            pong_timeout: Duration::from_millis(400),
            ..c
        },
        |b| b,
    )
    .await;
    let cookie = login(h.addr).await;
    let mut ws = ws_open(h.addr, &cookie, 0).await;

    // 아무 이벤트도 흐르지 않는다. 읽는 동안 라이브러리가 pong을 보낸다.
    let (pings, closed) = pings_while_reading(&mut ws, Duration::from_millis(1500)).await;
    assert!(!closed, "pong을 하는 연결은 끊지 않는다");
    assert!(
        (5..=14).contains(&pings),
        "0.15초 간격이면 1.5초 동안 ping이 열 번쯤 온다: {pings}번"
    );

    // pong 기다림(0.4초)보다 오래 지났어도 연결은 살아 있고 이벤트를 받는다.
    let seq = h
        .server
        .publisher()
        .publish("chat", json!({ "n": 1 }))
        .await
        .unwrap();
    let frame = next_frame(&mut ws).await;
    assert_eq!(seq_of(&frame), Some(seq));
}

// 검증: E3, C2
#[tokio::test]
async fn pong이_없으면_정한_시간_안에_연결을_끊는다() {
    let h = start_with(
        |c| GateConfig {
            keepalive_interval: Duration::from_millis(100),
            pong_timeout: Duration::from_millis(300),
            ..c
        },
        |b| b,
    )
    .await;
    let cookie = login(h.addr).await;
    let mut ws = ws_open(h.addr, &cookie, 0).await;

    // 1.5초 동안 읽지 않는다: pong이 나가지 않는다(시세가 흐르지 않는 죽은 연결).
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        closes_within(&mut ws, Duration::from_secs(3)).await,
        "ping 뒤 0.3초 안에 pong이 없으면 서버가 연결을 끊는다"
    );

    // 끊긴 연결이 발행을 막지 않고, 다시 이으면 번호로 이어 받는다.
    let seq = h
        .server
        .publisher()
        .publish("chat", json!({ "n": 1 }))
        .await
        .unwrap();
    let mut again = ws_open(h.addr, &cookie, 0).await;
    assert_eq!(read_seqs(&mut again, 1).await, [seq]);
}

// ───────────── C9 이벤트 종류 이름 ─────────────

/// 규칙에 어긋나는 이름들.
fn bad_kinds() -> Vec<String> {
    vec![
        String::new(),
        "Chat".to_string(),
        "feed status".to_string(),
        "1chat".to_string(),
        "_chat".to_string(),
        ".chat".to_string(),
        "chat!".to_string(),
        "가격".to_string(),
        "chat/x".to_string(),
        "a".repeat(EVENT_KIND_MAX_CHARS + 1),
    ]
}

// 검증: C2
#[test]
fn 종류_이름_규칙은_저장소와_같다() {
    for good in [
        "chat",
        "feed_status",
        "run.started",
        "message.delta",
        "a-b",
        "x9",
        "quotes_stale",
    ] {
        assert!(valid_event_kind(good), "{good}는 맞는 이름이다");
    }
    assert!(valid_event_kind(&"a".repeat(EVENT_KIND_MAX_CHARS)));
    for bad in bad_kinds() {
        assert!(!valid_event_kind(&bad), "{bad:?}는 규칙에 어긋난다");
    }
    assert!(!valid_event_kind(" chat "), "받은 글자 그대로 본다");
}

// 검증: C2, V6
#[tokio::test]
async fn 규칙에_어긋나는_종류는_발행하지_않고_번호를_쓰지_않는다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut live = ws_open(h.addr, &cookie, 0).await;
    let publisher = h.server.publisher();
    for bad in bad_kinds() {
        let result = publisher.publish(&bad, json!({ "n": 0 })).await;
        assert!(
            matches!(
                result,
                Err(GateError::Store(StoreError::InvalidEventKind { .. }))
            ),
            "{bad:?}: {result:?}"
        );
    }
    let longest = "a".repeat(EVENT_KIND_MAX_CHARS);
    let seq = publisher
        .publish(&longest, json!({ "n": 1 }))
        .await
        .unwrap();
    assert_eq!(seq, 1, "거부된 발행은 번호를 쓰지 않는다");
    let frame = next_frame(&mut live).await;
    assert_eq!(frame["kind"], json!(longest));
    assert_quiet(&mut live).await;
    assert_eq!(h.store.read_after(0, 100).unwrap().len(), 1);
}

// 검증: C2
#[tokio::test]
async fn 규칙에_어긋나는_종류의_지나가는_이벤트는_보내지도_기억하지도_않는다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut live = ws_open(h.addr, &cookie, 0).await;
    let publisher = h.server.publisher();
    for bad in bad_kinds() {
        publisher.publish_transient(&bad, json!({ "bad": true }));
    }
    publisher.publish_transient("quotes", json!({ "n": 1 }));
    let frame = next_frame(&mut live).await;
    assert_eq!(frame["kind"], json!("quotes"), "어긋난 이름은 건너뛴다");
    assert_eq!(frame["payload"], json!({ "n": 1 }));
    assert_quiet(&mut live).await;

    // 새로 연결한 화면도 맞는 이름의 것만 받는다.
    let mut late = ws_open(h.addr, &cookie, 0).await;
    let first = next_frame(&mut late).await;
    assert_eq!(first["kind"], json!("quotes"));
    assert_quiet(&mut late).await;
}

// ───────────── 참고 8: 시각 칸 · 저장된 이름 ─────────────

// 검증: C2, E3, V6
#[tokio::test]
async fn 발행한_이벤트는_시각_칸에_시각과_함께_내용_그대로_저장된다() {
    let h = start().await;
    h.clock.set(T0 + 42);
    let publisher = h.server.publisher();
    let seq = publisher
        .publish("chat", json!({ "text": "안녕" }))
        .await
        .unwrap();
    let stored = h.store.read_events(EventRange::default(), 10).unwrap();
    assert_eq!(
        stored,
        [TimedEvent {
            seq,
            kind: "chat".to_string(),
            at: Some(UnixMillis(T0 + 42)),
            payload: json!({ "text": "안녕" }),
        }],
        "시각 봉투로 싸지 않고 시각은 시각 칸에 둔다"
    );
    // 시각으로 고를 수 있다(저장소의 시각 조건이 이 이벤트를 안다).
    let range = EventRange {
        after_at: Some(UnixMillis(T0 + 41)),
        before_at: Some(UnixMillis(T0 + 43)),
        ..EventRange::default()
    };
    assert_eq!(h.store.read_events(range, 10).unwrap().len(), 1);
}

// 검증: C2, E3
#[tokio::test]
async fn 프레임의_at은_저장소의_시각_칸에서_온다() {
    let h = start().await;
    // 다른 곳이 저장소에 바로 쌓은 이벤트: 하나는 시각과 함께, 하나는 시각 없이.
    h.store
        .append_at("chat", &json!({ "n": 1 }), UnixMillis(T0 + 7))
        .unwrap();
    h.store.append("note", &json!({ "n": 2 })).unwrap();
    let cookie = login(h.addr).await;
    let mut ws = ws_open(h.addr, &cookie, 0).await;
    let first = next_frame(&mut ws).await;
    assert_eq!(seq_of(&first), Some(1));
    assert_eq!(first["at"], json!(T0 + 7), "시각 칸의 시각이다");
    assert_eq!(first["payload"], json!({ "n": 1 }));
    let second = next_frame(&mut ws).await;
    assert_eq!(seq_of(&second), Some(2));
    assert_eq!(second["at"], Value::Null, "시각 칸이 비었으면 null이다");
    assert_eq!(second["payload"], json!({ "n": 2 }));

    // 발행한 것도 다시 보낼 때 시각 칸의 시각이 나온다(다시 이어도 같다).
    h.clock.set(T0 + 99);
    let seq = h
        .server
        .publisher()
        .publish("chat", json!({ "n": 3 }))
        .await
        .unwrap();
    let live = next_frame(&mut ws).await;
    assert_eq!(live["at"], json!(T0 + 99));
    let mut again = ws_open(h.addr, &cookie, seq - 1).await;
    let replayed = next_frame(&mut again).await;
    assert_eq!(replayed, live, "다시 보내도 같은 프레임이다");
}

// 검증: C2, E3
#[tokio::test]
async fn 앞뒤_공백이_있는_종류는_저장된_이름으로_보낸다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut live = ws_open(h.addr, &cookie, 0).await;
    // 연결이 받는 쪽에 올라갈 때까지 기다린다(그 뒤의 이벤트는 저장소를 거치지 않고 바로 간다).
    assert_quiet(&mut live).await;
    let seq = h
        .server
        .publisher()
        .publish("  chat ", json!({ "n": 1 }))
        .await
        .unwrap();
    let stored = h.store.read_after(0, 10).unwrap();
    assert_eq!(stored[0].kind, "chat", "저장소는 앞뒤 공백을 떼어 저장한다");
    let frame = next_frame(&mut live).await;
    assert_eq!(seq_of(&frame), Some(seq));
    assert_eq!(
        frame["kind"],
        json!("chat"),
        "바로 보내는 프레임도 저장된 이름이다"
    );
    let mut again = ws_open(h.addr, &cookie, 0).await;
    assert_eq!(next_frame(&mut again).await, frame, "다시 보내도 같다");
}

// 검증: V6, E3, C2
#[tokio::test]
async fn 버전_2에서_올라온_옛_봉투_줄은_시각과_내용이_예전과_같다() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gate.sqlite");
    std::fs::copy(STORE_V2_FIXTURE, &db).unwrap();
    let store = Store::open(&db).unwrap();
    let clock = Arc::new(TestClock(AtomicI64::new(T0 + 5000)));
    let hits = Arc::new(AtomicUsize::new(0));
    let server = serve_on(&store, &clock, &hits, |c| c, |b| b).await;
    let addr = server.local_addr();
    let cookie = login(addr).await;

    // (번호, 종류, 시각, 내용). 봉투 모양인 2 · 3 · 7번은 풀어서 보낸다. 나머지는 내용 그대로다.
    let expected = [
        (1, "chat", Value::Null, json!({ "n": 1, "text": "장 시작" })),
        (2, "feed_status", json!(V2_T0 + 1000), json!({ "ok": true })),
        (3, "Old.Kind", json!(V2_T0 + 1010), Value::Null),
        (4, "quotes stale", Value::Null, json!("글자 하나만")),
        (
            5,
            "chat",
            Value::Null,
            json!({ "at": "어제", "payload": 1 }),
        ),
        (6, "chat", Value::Null, json!({ "at": V2_T0 + 1020 })),
        (
            7,
            "trade",
            json!(V2_T0 + 990),
            json!({ "amount": 9_007_199_254_740_993_i64 }),
        ),
    ];
    let mut first_pass = Vec::new();
    for _ in 0..2 {
        let mut ws = ws_open(addr, &cookie, 0).await;
        let mut frames = Vec::new();
        for (seq, kind, at, payload) in &expected {
            let frame = next_frame(&mut ws).await;
            assert_eq!(seq_of(&frame), Some(*seq));
            assert_eq!(frame["kind"], json!(kind), "옛 이름도 저장된 그대로다");
            assert_eq!(&frame["at"], at, "{seq}번의 시각");
            assert_eq!(&frame["payload"], payload, "{seq}번의 내용");
            frames.push(frame);
        }
        if first_pass.is_empty() {
            first_pass = frames;
        } else {
            assert_eq!(frames, first_pass, "다시 보내도 시각과 내용이 같다");
        }
    }

    // 올라온 뒤 새로 발행한 것은 시각 칸에 쓰고 번호가 이어진다.
    let seq = server
        .publisher()
        .publish("chat", json!({ "n": 8 }))
        .await
        .unwrap();
    assert_eq!(seq, 8);
    let last = store
        .read_events(
            EventRange {
                after_seq: 7,
                ..EventRange::default()
            },
            10,
        )
        .unwrap();
    assert_eq!(last[0].at, Some(UnixMillis(T0 + 5000)));
    assert_eq!(last[0].payload, json!({ "n": 8 }));
    drop(dir);
}
