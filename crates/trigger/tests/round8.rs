//! trigger 8차 시험(백로그 U 3번 "본 것 기억 기록 시점"): 상한으로 버린 자료 사건을 수로만 알리지 않고 사건 그대로
//! 돌려준다([`Trigger::take_returned_data`]). 부르는 쪽(server)은 그것을 "본 것"에서 빼 다음에 다시 넘긴다.
//!
//! - 돌려주는 때: 그 사건이 들던 묶음이 깨우기로 나간 뒤(또는 한 tick 합 상한으로 버린 그때). 꽉 찬 묶음에 다시
//!   넣어 곧바로 또 버리지 않게.
//! - 돌려준 사건은 중복 표에서 빠진다(다시 넣으면 받는다).
//! - 돌려주지 않는 것: 전체 정지로 버린 것(F4 7b2 — 의도된 버림), 구독을 꺼 버린 것(7차 — 같은 사건으로 다시 깨우지
//!   않는다), 실패한 깨우기에 실렸던 것(6차 — 다시 보내지 않는다).
//!
//! 잠긴 `tests/suite.rs` 묶음을 고치지 않으려고 따로 둔 시험 바이너리다. 시각은 가짜 시계로만 움직인다.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{
    Clock, FakeAdapter, Market, MarketAdapter, MarketCalendar, Session, SessionKind, Symbol,
    TradingDay,
};
use pi_trigger::{
    DataChange, DataKind, DataPush, MemoryStore, Monotonic, Ports, StopSignal, Trigger,
    TriggerConfig, TriggerEvent, Wake, WakeError, Waker,
};

struct Clockwork {
    wall: Mutex<DateTime<Utc>>,
    mono: Mutex<Duration>,
}

impl Clockwork {
    fn advance(&self, by: Duration) {
        *self.wall.lock().unwrap() += chrono::Duration::from_std(by).unwrap();
        *self.mono.lock().unwrap() += by;
    }
}

#[async_trait]
impl Clock for Clockwork {
    fn now(&self) -> DateTime<Utc> {
        *self.wall.lock().unwrap()
    }
    async fn sleep(&self, duration: Duration) {
        self.advance(duration);
    }
}

impl Monotonic for Clockwork {
    fn elapsed(&self) -> Duration {
        *self.mono.lock().unwrap()
    }
}

#[derive(Default)]
struct Switch(AtomicBool);

impl StopSignal for Switch {
    fn is_stopped(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct Wakes {
    seen: Mutex<Vec<Wake>>,
    fail: AtomicBool,
}

#[async_trait]
impl Waker for Wakes {
    async fn wake(&self, wake: Wake) -> Result<(), WakeError> {
        self.seen.lock().unwrap().push(wake);
        if self.fail.load(Ordering::SeqCst) {
            return Err(WakeError("시험: 깨우기 실패".to_string()));
        }
        Ok(())
    }
}

fn day(d: u32) -> TradingDay {
    let date = NaiveDate::from_ymd_opt(2026, 11, d).unwrap();
    TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: date.and_hms_opt(0, 0, 0).unwrap().and_utc(),
            end: date.and_hms_opt(6, 30, 0).unwrap().and_utc(),
        }],
    }
}

/// 2026-11-16(월) 국내 정규장 00:00~06:30 UTC, 앞뒤 영업일.
fn calendar() -> MarketCalendar {
    MarketCalendar {
        market: Market::Kr,
        today: day(16),
        previous_business_day: day(13),
        next_business_day: day(17),
    }
}

struct Bench {
    clock: Arc<Clockwork>,
    wakes: Arc<Wakes>,
    stop: Arc<Switch>,
    trigger: Trigger,
}

fn bench() -> Bench {
    let at = Utc.with_ymd_and_hms(2026, 11, 16, 1, 0, 0).unwrap();
    let clock = Arc::new(Clockwork {
        wall: Mutex::new(at),
        mono: Mutex::new(Duration::from_secs(1_000)),
    });
    let adapter = FakeAdapter::new(Market::Kr);
    adapter.set_calendar(calendar());
    let market: Arc<dyn MarketAdapter> = Arc::new(adapter);
    let wakes = Arc::new(Wakes::default());
    let stop = Arc::new(Switch::default());
    let ports = Ports {
        clock: clock.clone(),
        monotonic: clock.clone(),
        markets: vec![market],
        store: Arc::new(MemoryStore::default()),
        waker: wakes.clone(),
        stop: stop.clone(),
    };
    let trigger = Trigger::new(TriggerConfig::new(vec![], vec![]), ports).unwrap();
    Bench {
        clock,
        wakes,
        stop,
        trigger,
    }
}

/// 같은 묶음(국내 · 005930)의 뉴스 하나. `n`이 작을수록 먼저 난 것이다.
fn news(n: u32, base: DateTime<Utc>) -> DataChange {
    let at = base + chrono::Duration::seconds(i64::from(n));
    DataChange {
        kind: DataKind::News,
        source: "naver".to_string(),
        id: format!("news-{n:02}"),
        revision: None,
        market: Market::Kr,
        symbol: Some(Symbol::new("005930")),
        title: format!("뉴스 {n:02}"),
        published_at: at,
        learned_at: at,
    }
}

fn woken_ids(bench: &Bench) -> Vec<String> {
    bench
        .wakes
        .seen
        .lock()
        .unwrap()
        .iter()
        .flat_map(|wake| wake.events.iter())
        .filter_map(|event| match event {
            TriggerEvent::Data { change, .. } => Some(change.id.clone()),
            _ => None,
        })
        .collect()
}

fn ids(changes: &[DataChange]) -> Vec<String> {
    changes.iter().map(|change| change.id.clone()).collect()
}

async fn tick(bench: &Bench) {
    bench.trigger.tick().await.unwrap();
    bench.trigger.settle().await.unwrap();
}

/// 한 묶음에 22개(상한 20)를 넣는다. 가장 이른 둘(`news-00` · `news-01`)이 버려진다.
fn overfill(bench: &Bench) {
    let base = bench.clock.now() - chrono::Duration::minutes(10);
    for n in 0..22 {
        assert_eq!(bench.trigger.push_data(news(n, base)), DataPush::Queued);
    }
}

// 검증: F2 E14
#[tokio::test]
async fn 묶음_상한으로_버린_사건은_묶음이_나간_뒤_돌려주고_다시_넣으면_받는다() {
    let b = bench();
    overfill(&b);
    assert!(
        b.trigger.take_returned_data().is_empty(),
        "묶음이 아직 나가지 않았으면 돌려주지 않는다(꽉 찬 묶음에 다시 넣어 또 버리지 않게)"
    );
    b.clock.advance(Duration::from_secs(31));
    tick(&b).await;
    let woken = woken_ids(&b);
    assert_eq!(woken.len(), 20, "한 깨우기 상한");
    assert!(!woken.contains(&"news-00".to_string()));

    let returned = b.trigger.take_returned_data();
    assert_eq!(
        ids(&returned),
        vec!["news-00", "news-01"],
        "버린 것을 이른 순으로 돌려준다"
    );
    assert!(
        b.trigger.take_returned_data().is_empty(),
        "한 번만 돌려준다"
    );

    for change in returned {
        assert_eq!(
            b.trigger.push_data(change),
            DataPush::Queued,
            "돌려준 사건은 중복 표에서 빠져 다시 받는다"
        );
    }
    b.clock.advance(Duration::from_secs(31));
    tick(&b).await;
    let woken = woken_ids(&b);
    assert!(woken.contains(&"news-00".to_string()), "{woken:?}");
    assert!(woken.contains(&"news-01".to_string()), "{woken:?}");
}

// 검증: F2 E14 F4
#[tokio::test]
async fn 전체_정지로_버린_사건은_돌려주지_않고_중복_표도_그대로다() {
    let b = bench();
    overfill(&b);
    b.stop.0.store(true, Ordering::SeqCst);
    b.clock.advance(Duration::from_secs(31));
    tick(&b).await;
    assert!(woken_ids(&b).is_empty(), "전체 정지 중에는 깨우지 않는다");
    assert!(
        b.trigger.take_returned_data().is_empty(),
        "전체 정지로 버린 것은 의도된 버림이라 돌려주지 않는다(묶음 상한으로 먼저 버린 것도 그 묶음과 함께 버린다)"
    );
    let base = b.clock.now() - chrono::Duration::minutes(10) - chrono::Duration::seconds(31);
    assert_eq!(b.trigger.push_data(news(5, base)), DataPush::Duplicate);
}

// 검증: F2 E14
#[tokio::test]
async fn 구독을_꺼_버린_사건과_실패한_깨우기의_사건은_돌려주지_않는다() {
    let b = bench();
    overfill(&b);
    assert_eq!(b.trigger.discard_data(&[DataKind::News]), 20);
    b.clock.advance(Duration::from_secs(31));
    tick(&b).await;
    assert!(
        b.trigger.take_returned_data().is_empty(),
        "구독을 꺼 버린 묶음의 앞서 버린 것도 돌려주지 않는다"
    );

    let b = bench();
    b.wakes.fail.store(true, Ordering::SeqCst);
    let base = b.clock.now() - chrono::Duration::minutes(10);
    for n in 0..3 {
        b.trigger.push_data(news(n, base));
    }
    b.clock.advance(Duration::from_secs(31));
    tick(&b).await;
    assert_eq!(woken_ids(&b).len(), 3, "깨우기는 갔다(실패)");
    assert!(
        b.trigger.take_returned_data().is_empty(),
        "실패한 깨우기의 사건은 다시 보내지 않는다(6차 그대로)"
    );
}
