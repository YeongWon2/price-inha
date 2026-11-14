//! 5차 작업: 4차 AI 검토 되돌림.
//!
//! 근거: `docs/dev/modules/gate.md` "5차 작업" 1~5, `pi_store::Store::legacy_envelope_until`의 문서 주석.
//! - 1번(2026-11-14 사용자 결정 "경계 번호로"): 저장소의 옛 형식 경계 번호 **이하**의 줄만 옛 봉투
//!   모양(정확히 두 칸 · `at` 정수 · 시각 칸이 비었거나 내용의 `at`과 같다)으로 푼다. 그 뒤의 줄은
//!   모양이 같아도 내용 그대로다. 경계가 없으면 아무것도 풀지 않는다. 바로 보내기 · 다시 보내기 둘 다.
//! - 2번: 지나가는 이벤트의 이름도 저장소처럼 앞뒤 공백(Python `str.isspace`)을 떼고 검사하고,
//!   뗀 이름으로 보내고 기억한다(원본 Hermes `gateway/hosted_rooms_common.py:23-32`).
//! - 3번: 들어온 프레임(pong)을 기한 판단보다 먼저 읽는다. keepalive 간격 · pong 기한의 0을 받지 않는다.
//! - 4번(413)은 `price-inha`의 `round5/`가 본다. 5번은 저장소에 그런 API가 없어 이번에는 하지 않는다.
//!
//! 픽스처(`pi-store`의 테스트 자료)는 임시 폴더에 복사해서 연다. 원본 파일은 읽기만 한다.
//! - `v2.db`: 이벤트 7개, 경계 7. 봉투 모양은 2 · 3 · 7번.
//! - `v3.db`: `v2.db`를 버전 3으로 올린 뒤 4개를 더 쌓은 것. 이벤트 11개, 경계 10. 옛 봉투 줄은 2 · 3 · 7 · 10번.
//! - `v3_plain.db`: 처음부터 버전 3. 이벤트 4개, 경계 3. 3번은 시각 칸이 빈 봉투 줄이다.

use super::*;
use futures::SinkExt;
use pi_gate::SystemClock;
use pi_store::EventRange;

// ───────────── 도우미 ─────────────

const FIXTURE_V2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../store/tests/fixtures/v2.db");
const FIXTURE_V3: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../store/tests/fixtures/v3.db");
const FIXTURE_V3_PLAIN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../store/tests/fixtures/v3_plain.db"
);

/// 픽스처의 시각 기준(`pi-store` 테스트의 `t(n)`).
fn ft(n: i64) -> i64 {
    1_800_000_000_000 + n
}

/// 픽스처를 임시 폴더에 복사해 연다.
fn open_copy(fixture: &str) -> (TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gate.sqlite");
    std::fs::copy(fixture, &db).unwrap();
    let store = Store::open(&db).unwrap();
    (dir, store)
}

/// 이미 있는 저장소로 테스트 시계 서버를 띄운다.
async fn serve_fixed(store: &Store, at: i64) -> (GateServer, Arc<TestClock>) {
    let clock = Arc::new(TestClock(AtomicI64::new(at)));
    let hits = Arc::new(AtomicUsize::new(0));
    let server = serve_on(store, &clock, &hits, |c| c, |b| b).await;
    (server, clock)
}

/// 봉투 모양 내용: 정확히 두 칸, `at`은 정수.
fn envelope(at: i64, payload: Value) -> Value {
    json!({ "at": at, "payload": payload })
}

/// 연결이 받는 쪽에 올라간 뒤의 실시간 연결(그 뒤의 발행은 바로 보내기로 온다).
async fn live_after(addr: SocketAddr, cookie: &str, last_seq: u64) -> Ws {
    let mut ws = ws_open(addr, cookie, last_seq).await;
    assert_quiet(&mut ws).await;
    ws
}

/// 번호 `seq`인 저장된 프레임을 다시 보내기로 받는다.
async fn replayed(addr: SocketAddr, cookie: &str, seq: u64) -> Value {
    let mut ws = ws_open(addr, cookie, seq - 1).await;
    let frame = next_frame(&mut ws).await;
    assert_eq!(seq_of(&frame), Some(seq), "다시 보내기는 {seq}번부터다");
    frame
}

/// 번호 · 시각 · 내용이 그대로인 프레임인지 본다.
fn assert_frame(frame: &Value, seq: u64, kind: &str, at: &Value, payload: &Value, what: &str) {
    assert_eq!(seq_of(frame), Some(seq), "{what}: 번호");
    assert_eq!(frame["kind"], json!(kind), "{what}: 종류");
    assert_eq!(&frame["at"], at, "{what}: 시각");
    assert_eq!(&frame["payload"], payload, "{what}: 내용");
}

/// 테스트 시계로 봉투 모양 내용을 발행하고, 바로 보내기 · 다시 보내기 모두 내용 그대로인지 본다.
/// 발행 시각과 내용의 `at`이 같다(4차가 옛 봉투로 착각해 벗기던 모양).
async fn assert_published_envelope_intact(
    server: &GateServer,
    clock: &TestClock,
    store: &Store,
    at: i64,
    what: &str,
) -> u64 {
    let addr = server.local_addr();
    let cookie = login(addr).await;
    let last = store
        .read_after(0, 100_000)
        .unwrap()
        .last()
        .map_or(0, |e| e.seq);
    let mut live = live_after(addr, &cookie, last).await;
    clock.set(at);
    let content = envelope(at, json!({ "n": 1, "text": "새 발행" }));
    let seq = server
        .publisher()
        .publish("chat", content.clone())
        .await
        .unwrap();
    // 저장된 줄은 시각 칸과 내용의 `at`이 같다(옛 봉투 모양 그대로다).
    let saved = store
        .read_events(
            EventRange {
                after_seq: seq - 1,
                ..EventRange::default()
            },
            1,
        )
        .unwrap();
    assert_eq!(saved[0].at, Some(UnixMillis(at)), "{what}: 시각 칸");
    assert_eq!(saved[0].payload, content, "{what}: 저장된 내용");

    let direct = next_frame(&mut live).await;
    assert_frame(
        &direct,
        seq,
        "chat",
        &json!(at),
        &content,
        &format!("{what} 바로 보내기"),
    );
    let again = replayed(addr, &cookie, seq).await;
    assert_frame(
        &again,
        seq,
        "chat",
        &json!(at),
        &content,
        &format!("{what} 다시 보내기"),
    );
    seq
}

// ───────────── 1번: 옛 봉투는 경계 번호 이하에서만 푼다 ─────────────

// 검증: V6, E3, C2
#[tokio::test]
async fn 고정_시계로_발행한_봉투_모양_내용은_바로_보내기와_다시_보내기에서_그대로_온다() {
    let h = start().await;
    assert_eq!(
        h.store.legacy_envelope_until().unwrap(),
        None,
        "새 저장소는 경계가 없다"
    );
    assert_published_envelope_intact(&h.server, &h.clock, &h.store, T0 + 42, "새 저장소").await;

    // 내용 안의 `payload`가 무엇이든(null · 글자 · 숫자) 그대로다.
    let cookie = login(h.addr).await;
    for (i, inner) in [Value::Null, json!("글자"), json!(7), json!([1, 2])]
        .into_iter()
        .enumerate()
    {
        let at = T0 + 100 + i as i64;
        h.clock.set(at);
        let content = envelope(at, inner);
        let seq = h
            .server
            .publisher()
            .publish("note", content.clone())
            .await
            .unwrap();
        let again = replayed(h.addr, &cookie, seq).await;
        assert_frame(
            &again,
            seq,
            "note",
            &json!(at),
            &content,
            "안쪽 내용이 무엇이든",
        );
    }
}

// 검증: V6, E3, C2
#[tokio::test]
async fn 실제_시계로_봉투_모양_내용을_여러_번_발행해도_다_그대로_온다() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
    let (public, protected) = counted_routes(&Arc::new(AtomicUsize::new(0)));
    let server = tokio::time::timeout(
        STEP,
        Gate::builder(store.clone(), GateConfig::local(0))
            .clock(Arc::new(SystemClock) as Arc<dyn Clock>)
            .public_routes(public)
            .protected_routes(protected)
            .build()
            .serve(),
    )
    .await
    .expect("서버가 시간 안에 떠야 한다")
    .expect("서버가 떠야 한다");
    let addr = server.local_addr();
    let cookie = login(addr).await;
    let mut live = live_after(addr, &cookie, 0).await;

    let publisher = server.publisher();
    let mut sent = Vec::new();
    for i in 0..50 {
        let content = envelope(SystemClock.now().0, json!({ "i": i }));
        let seq = publisher.publish("chat", content.clone()).await.unwrap();
        sent.push((seq, content));
    }
    // 이 테스트가 정말 그 모양을 다루는지: 시각 칸과 내용의 `at`이 같은 줄이 있다.
    let saved = store.read_events(EventRange::default(), 100).unwrap();
    let same = saved
        .iter()
        .filter(|e| e.at.map(|at| json!(at.0)) == Some(e.payload["at"].clone()))
        .count();
    assert!(
        same > 0,
        "발행 시각과 내용의 at이 같은 줄이 하나는 있어야 한다"
    );

    for (seq, content) in &sent {
        let frame = next_frame(&mut live).await;
        assert_eq!(seq_of(&frame), Some(*seq));
        assert_eq!(
            &frame["payload"], content,
            "{seq}번 바로 보내기는 내용 그대로다"
        );
    }
    let mut again = ws_open(addr, &cookie, 0).await;
    for (seq, content) in &sent {
        let frame = next_frame(&mut again).await;
        assert_eq!(seq_of(&frame), Some(*seq));
        assert_eq!(
            &frame["payload"], content,
            "{seq}번 다시 보내기는 내용 그대로다"
        );
    }
    server.shutdown().await.unwrap();
}

// 검증: V6, E3, C2
#[tokio::test]
async fn 경계가_없는_새_저장소에서는_옛_봉투_모양이어도_풀지_않는다() {
    let h = start().await;
    assert_eq!(h.store.legacy_envelope_until().unwrap(), None);
    // 다른 곳이 저장소에 바로 쌓은 봉투 모양 줄: 시각 칸이 빈 것, 내용의 `at`과 같은 시각인 것.
    let blank = envelope(T0 + 1, json!({ "ok": true }));
    let same = envelope(T0 + 2, json!({ "ok": false }));
    h.store.append("feed_status", &blank).unwrap();
    h.store
        .append_at("feed_status", &same, UnixMillis(T0 + 2))
        .unwrap();
    let cookie = login(h.addr).await;
    for _ in 0..2 {
        let mut ws = ws_open(h.addr, &cookie, 0).await;
        let first = next_frame(&mut ws).await;
        assert_frame(
            &first,
            1,
            "feed_status",
            &Value::Null,
            &blank,
            "시각 칸이 빈 줄",
        );
        let second = next_frame(&mut ws).await;
        assert_frame(
            &second,
            2,
            "feed_status",
            &json!(T0 + 2),
            &same,
            "같은 시각의 줄",
        );
    }
    // 다시 켜도(같은 저장소) 그대로다.
    h.server.shutdown().await.unwrap();
    let store = Store::open(&h.db).unwrap();
    let (server, _clock) = serve_fixed(&store, T0).await;
    let addr = server.local_addr();
    let cookie = login(addr).await;
    let mut ws = ws_open(addr, &cookie, 0).await;
    assert_frame(
        &next_frame(&mut ws).await,
        1,
        "feed_status",
        &Value::Null,
        &blank,
        "다시 켠 뒤",
    );
    assert_frame(
        &next_frame(&mut ws).await,
        2,
        "feed_status",
        &json!(T0 + 2),
        &same,
        "다시 켠 뒤",
    );
    drop(h.dir);
}

// 검증: V6, E3, C2
#[tokio::test]
async fn 버전_2_픽스처는_경계_안의_옛_줄만_풀고_경계_뒤의_같은_모양은_그대로_보낸다() {
    let (dir, store) = open_copy(FIXTURE_V2);
    assert_eq!(
        store.legacy_envelope_until().unwrap(),
        Some(7),
        "버전 2의 경계는 마지막 번호다"
    );
    let (server, clock) = serve_fixed(&store, T0).await;
    let addr = server.local_addr();

    // 경계 뒤: 발행한 봉투 모양(시각이 같다)과, 바로 쌓은 시각 칸이 빈 봉투 모양.
    let seq =
        assert_published_envelope_intact(&server, &clock, &store, ft(5000), "버전 2 경계 뒤").await;
    assert_eq!(seq, 8);
    let blank = envelope(ft(5001), json!({ "ok": true }));
    let blank_seq = store.append("feed_status", &blank).unwrap();
    assert_eq!(blank_seq, 9);

    // 경계 안의 옛 줄은 예전처럼 풀린다(2 · 3 · 7번). 다시 이어도 같다.
    let cookie = login(addr).await;
    let expected = [
        (2, "feed_status", json!(ft(1000)), json!({ "ok": true })),
        (3, "Old.Kind", json!(ft(1010)), Value::Null),
        (
            7,
            "trade",
            json!(ft(990)),
            json!({ "amount": 9_007_199_254_740_993_i64 }),
        ),
    ];
    for (seq, kind, at, payload) in &expected {
        let frame = replayed(addr, &cookie, *seq).await;
        assert_frame(&frame, *seq, kind, at, payload, &format!("경계 안 {seq}번"));
    }
    let frame = replayed(addr, &cookie, 9).await;
    assert_frame(
        &frame,
        9,
        "feed_status",
        &Value::Null,
        &blank,
        "경계 뒤 시각 칸이 빈 줄",
    );
    drop(dir);
}

// 검증: V6, E3, C2
#[tokio::test]
async fn 버전_3_픽스처는_경계_10까지만_풀고_그_뒤는_그대로_보낸다() {
    let (dir, store) = open_copy(FIXTURE_V3);
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(10));
    let (server, clock) = serve_fixed(&store, T0).await;
    let addr = server.local_addr();
    let cookie = login(addr).await;

    // (번호, 종류, 시각, 내용). 2 · 3 · 7 · 10번은 풀고 나머지는 내용 그대로다.
    let expected = [
        (1, "chat", Value::Null, json!({ "n": 1, "text": "장 시작" })),
        (2, "feed_status", json!(ft(1000)), json!({ "ok": true })),
        (3, "Old.Kind", json!(ft(1010)), Value::Null),
        (4, "quotes stale", Value::Null, json!("글자 하나만")),
        (
            5,
            "chat",
            Value::Null,
            json!({ "at": "어제", "payload": 1 }),
        ),
        (6, "chat", Value::Null, json!({ "at": ft(1020) })),
        (
            7,
            "trade",
            json!(ft(990)),
            json!({ "amount": 9_007_199_254_740_993_i64 }),
        ),
        (8, "chat", Value::Null, json!({ "n": 8, "text": "올린 뒤" })),
        (9, "feed_status", json!(ft(2000)), json!({ "ok": false })),
        (10, "feed_status", json!(ft(2010)), json!({ "ok": true })),
        (
            11,
            "chat",
            json!(ft(2020)),
            json!({ "n": 11, "text": "마지막" }),
        ),
    ];
    for _ in 0..2 {
        let mut ws = ws_open(addr, &cookie, 0).await;
        for (seq, kind, at, payload) in &expected {
            let frame = next_frame(&mut ws).await;
            assert_frame(&frame, *seq, kind, at, payload, &format!("버전 3 {seq}번"));
        }
    }
    let seq =
        assert_published_envelope_intact(&server, &clock, &store, ft(6000), "버전 3 경계 뒤").await;
    assert_eq!(seq, 12);
    drop(dir);
}

// 검증: V6, E3, C2
#[tokio::test]
async fn 처음부터_버전_3인_픽스처는_경계_3까지만_풀고_그_뒤는_그대로_보낸다() {
    let (dir, store) = open_copy(FIXTURE_V3_PLAIN);
    assert_eq!(store.legacy_envelope_until().unwrap(), Some(3));
    let (server, clock) = serve_fixed(&store, T0).await;
    let addr = server.local_addr();
    let cookie = login(addr).await;

    // 3번은 시각 칸이 빈 옛 봉투 줄이라 내용의 `at`이 프레임의 시각이 된다.
    let expected = [
        (1, Value::Null, json!({ "n": 1 })),
        (2, json!(ft(3000)), json!({ "n": 2 })),
        (3, json!(ft(3010)), json!({ "ok": true })),
        (4, json!(ft(3020)), json!({ "n": 4 })),
    ];
    let kinds = ["chat", "chat", "feed_status", "chat"];
    for _ in 0..2 {
        let mut ws = ws_open(addr, &cookie, 0).await;
        for ((seq, at, payload), kind) in expected.iter().zip(kinds) {
            let frame = next_frame(&mut ws).await;
            assert_frame(
                &frame,
                *seq,
                kind,
                at,
                payload,
                &format!("버전 3(새로 만든 것) {seq}번"),
            );
        }
    }

    // 경계 뒤: 시각 칸이 빈 봉투 모양 줄(3번과 같은 모양)도, 발행한 봉투 모양도 그대로다.
    let blank = envelope(ft(3030), json!({ "ok": false }));
    let blank_seq = store.append("feed_status", &blank).unwrap();
    assert_eq!(blank_seq, 5);
    let frame = replayed(addr, &cookie, 5).await;
    assert_frame(
        &frame,
        5,
        "feed_status",
        &Value::Null,
        &blank,
        "경계 뒤 시각 칸이 빈 줄",
    );
    let seq = assert_published_envelope_intact(
        &server,
        &clock,
        &store,
        ft(7000),
        "버전 3(새로 만든 것) 경계 뒤",
    )
    .await;
    assert_eq!(seq, 6);
    drop(dir);
}

// ───────────── 2번: 지나가는 이벤트의 이름도 앞뒤 공백을 뗀다 ─────────────

// 검증: C2
#[tokio::test]
async fn 지나가는_이벤트의_이름은_앞뒤_공백을_떼고_검사해_뗀_이름으로_보낸다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let mut live = live_after(h.addr, &cookie, 0).await;
    let publisher = h.server.publisher();
    publisher.publish_transient("  quotes ", json!({ "n": 1 }));
    let frame = next_frame(&mut live).await;
    assert_eq!(frame["kind"], json!("quotes"), "뗀 이름으로 보낸다");
    assert_eq!(frame["payload"], json!({ "n": 1 }));
    assert_eq!(seq_of(&frame), None);

    // 저장소와 같은 판단: 같은 이름을 저장소에 쌓았을 때 받는지 · 어떤 이름으로 쌓는지와 같다.
    let probe_dir = tempfile::tempdir().unwrap();
    let probe = Store::open(probe_dir.path().join("probe.sqlite")).unwrap();
    let long = "a".repeat(pi_store::EVENT_KIND_MAX_CHARS);
    let names = [
        "\tfeed_status\n".to_string(),
        "\u{1c}run.started\u{1f}".to_string(),
        "\u{3000}message.delta\u{a0}".to_string(),
        format!("  {long}  "),
        format!(" {long}a "),
        "   ".to_string(),
        " Chat ".to_string(),
        " feed status ".to_string(),
        "\u{200b}chat".to_string(),
    ];
    for (i, name) in names.iter().enumerate() {
        let stored = probe
            .append(name, &json!({}))
            .ok()
            .map(|seq| probe.read_after(seq - 1, 1).unwrap()[0].kind.clone());
        publisher.publish_transient(name, json!({ "i": i }));
        publisher.publish_transient("sentinel", json!({ "after": i }));
        let first = next_frame(&mut live).await;
        match &stored {
            Some(kind) => {
                assert_eq!(
                    first["kind"],
                    json!(kind),
                    "{name:?}는 저장소처럼 {kind:?}로 보낸다"
                );
                assert_eq!(first["payload"], json!({ "i": i }));
                let next = next_frame(&mut live).await;
                assert_eq!(next["kind"], json!("sentinel"));
            }
            None => {
                assert_eq!(
                    first,
                    json!({ "kind": "sentinel", "payload": { "after": i }, "at": T0 }),
                    "{name:?}는 저장소가 받지 않으므로 보내지 않는다"
                );
            }
        }
    }
    drop(probe_dir);
}

// 검증: C2
#[tokio::test]
async fn 지나가는_이벤트는_뗀_이름으로_기억해_새_연결에_하나만_보낸다() {
    let h = start().await;
    let cookie = login(h.addr).await;
    let publisher = h.server.publisher();
    publisher.publish_transient(" quotes", json!({ "n": 1 }));
    publisher.publish_transient("quotes ", json!({ "n": 2 }));
    publisher.publish_transient("\tquotes\t", json!({ "n": 3 }));
    let mut late = ws_open(h.addr, &cookie, 0).await;
    let first = next_frame(&mut late).await;
    assert_eq!(first["kind"], json!("quotes"), "뗀 이름으로 기억한다");
    assert_eq!(first["payload"], json!({ "n": 3 }), "가장 최근 것 하나다");
    assert_quiet(&mut late).await;

    // 이름을 떼지 않고 보낸 것과 같은 자리를 덮는다.
    publisher.publish_transient("quotes", json!({ "n": 4 }));
    let mut later = ws_open(h.addr, &cookie, 0).await;
    let first = next_frame(&mut later).await;
    assert_eq!(first["kind"], json!("quotes"));
    assert_eq!(first["payload"], json!({ "n": 4 }));
    assert_quiet(&mut later).await;
}

// ───────────── 3번: pong을 받았는데 끊기지 않게 ─────────────

/// 큰 지나가는 이벤트의 내용(커널 소켓 버퍼를 넘겨 프레임 쓰기가 막히게 한다).
const BIG_FRAME_CHARS: usize = 4 * 1024 * 1024;
const BIG_FRAMES: usize = 6;

/// `wait` 동안 읽으며 온 ping 수와 닫혔는지를 센다(읽는 동안 pong이 나간다).
async fn pings_and_closed(ws: &mut Ws, wait: Duration) -> (usize, bool) {
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

// 검증: E3, C2
#[tokio::test]
async fn 프레임_쓰기가_밀려_기한을_넘겨도_그사이_들어온_pong이_있으면_끊지_않는다() {
    let interval = Duration::from_millis(150);
    let pong_timeout = Duration::from_millis(300);
    let h = start_with(
        |c| GateConfig {
            keepalive_interval: interval,
            pong_timeout,
            max_frame_bytes: 2 * BIG_FRAME_CHARS,
            ..c
        },
        |b| b,
    )
    .await;
    let cookie = login(h.addr).await;
    let mut ws = ws_open(h.addr, &cookie, 0).await;

    // 첫 ping을 읽는다. pong은 다음에 읽거나 비울 때 나간다(아직 나가지 않았다).
    let got_ping = tokio::time::timeout(STEP, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Ping(_))) => return true,
                Some(Ok(_)) => {}
                _ => return false,
            }
        }
    })
    .await
    .expect("ping이 시간 안에 와야 한다");
    assert!(got_ping, "연결이 열려 있어야 한다");

    // 보낼 것을 먼저 채운다: 화면이 읽지 않는 동안 서버의 프레임 쓰기가 막힌다.
    let blob = "a".repeat(BIG_FRAME_CHARS);
    let publisher = h.server.publisher();
    for i in 0..BIG_FRAMES {
        publisher.publish_transient("bulk", json!({ "i": i, "blob": blob }));
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    // 이제 pong을 보낸다(기한 안이다). 그 뒤 기한을 훌쩍 넘겨 읽지 않는다.
    ws.flush().await.expect("pong을 보낼 수 있어야 한다");
    tokio::time::sleep(pong_timeout * 3).await;

    // 밀린 프레임을 다 받는다.
    for i in 0..BIG_FRAMES {
        let text = next_text(&mut ws, STEP)
            .await
            .expect("프레임이 시간 안에 와야 한다")
            .unwrap_or_else(|| {
                panic!("pong을 했으므로 연결이 열려 있어야 한다({i}번째 큰 프레임)")
            });
        let frame: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(frame["payload"]["i"], json!(i));
    }
    // 그 뒤에도 살아 있다: ping이 계속 오고 닫히지 않는다.
    let (pings, closed) = pings_and_closed(&mut ws, pong_timeout * 4).await;
    assert!(!closed, "기한 전에 pong이 들어와 있었으므로 끊지 않는다");
    assert!(pings >= 2, "ping이 계속 와야 한다: {pings}번");
}

// 검증: E3
#[tokio::test]
async fn keepalive_간격이나_pong_기한이_0이면_받지_않는다() {
    for (field, config) in [
        (
            "keepalive_interval",
            GateConfig {
                keepalive_interval: Duration::ZERO,
                ..GateConfig::local(0)
            },
        ),
        (
            "pong_timeout",
            GateConfig {
                pong_timeout: Duration::ZERO,
                ..GateConfig::local(0)
            },
        ),
        (
            "keepalive_interval",
            GateConfig {
                keepalive_interval: Duration::ZERO,
                pong_timeout: Duration::ZERO,
                ..GateConfig::local(0)
            },
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
        let outcome = tokio::time::timeout(STEP, Gate::builder(store, config).build().serve())
            .await
            .expect("시간 안에 끝나야 한다");
        match outcome {
            Err(GateError::InvalidConfig { field: got }) => {
                assert_eq!(got, field, "어느 칸인지 알린다")
            }
            Err(other) => panic!("{field}이 0이면 InvalidConfig여야 한다: {other:?}"),
            Ok(server) => {
                let _ = server.shutdown().await;
                panic!("{field}이 0이면 서버가 뜨지 않아야 한다");
            }
        }
    }

    // 0이 아니면 아주 짧아도 받는다.
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("gate.sqlite")).unwrap();
    let config = GateConfig {
        keepalive_interval: Duration::from_millis(1),
        pong_timeout: Duration::from_millis(1),
        ..GateConfig::local(0)
    };
    let server = Gate::builder(store, config).build().serve().await.unwrap();
    server.shutdown().await.unwrap();
}
