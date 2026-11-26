//! trigger 7차 시험(백로그 R 묶음 C, server 10b-2 5번 "해제 직전 대기열에 든 사건 버리기"): 구독을 끈 종류의
//! 자료 사건이 묶음 · 장 밖 보관에 남아 있으면 [`Trigger::discard_data`]로 버려 그 뒤에 깨우지 않는다.
//!
//! 잠긴 `tests/suite.rs` 묶음을 고치지 않으려고 따로 둔 시험 바이너리다. 시각은 가짜 시계로만 움직인다.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use pi_market::{Clock, FakeAdapter, Market, MarketAdapter, MarketCalendar, Session, SessionKind, Symbol, TradingDay};
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

    fn set(&self, to: DateTime<Utc>) {
        let now = *self.wall.lock().unwrap();
        self.advance((to - now).to_std().unwrap());
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

struct Running;

impl StopSignal for Running {
    fn is_stopped(&self) -> bool {
        false
    }
}

#[derive(Default)]
struct Wakes(Mutex<Vec<Wake>>);

#[async_trait]
impl Waker for Wakes {
    async fn wake(&self, wake: Wake) -> Result<(), WakeError> {
        self.0.lock().unwrap().push(wake);
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
    trigger: Trigger,
}

fn bench(at: DateTime<Utc>) -> Bench {
    let clock = Arc::new(Clockwork {
        wall: Mutex::new(at),
        mono: Mutex::new(Duration::from_secs(1_000)),
    });
    let adapter = FakeAdapter::new(Market::Kr);
    adapter.set_calendar(calendar());
    let market: Arc<dyn MarketAdapter> = Arc::new(adapter);
    let wakes = Arc::new(Wakes::default());
    let ports = Ports {
        clock: clock.clone(),
        monotonic: clock.clone(),
        markets: vec![market],
        store: Arc::new(MemoryStore::default()),
        waker: wakes.clone(),
        stop: Arc::new(Running),
    };
    let trigger = Trigger::new(TriggerConfig::new(vec![], vec![]), ports).unwrap();
    Bench {
        clock,
        wakes,
        trigger,
    }
}

fn change(kind: DataKind, id: &str, at: DateTime<Utc>) -> DataChange {
    DataChange {
        kind,
        source: "fake".to_string(),
        id: id.to_string(),
        revision: None,
        market: Market::Kr,
        symbol: Some(Symbol::new("005930")),
        title: format!("{id} 제목"),
        published_at: at,
        learned_at: at,
    }
}

fn woken_ids(bench: &Bench) -> Vec<String> {
    bench
        .wakes
        .0
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

async fn tick(bench: &Bench) {
    bench.trigger.tick().await.unwrap();
    bench.trigger.settle().await.unwrap();
}

// 검증: F2 E14
#[tokio::test]
async fn 구독을_끈_종류의_묶인_사건은_버리고_남은_종류만_깨운다() {
    let t0 = Utc.with_ymd_and_hms(2026, 11, 16, 1, 0, 0).unwrap();
    let b = bench(t0);
    assert_eq!(b.trigger.push_data(change(DataKind::News, "news-1", t0)), DataPush::Queued);
    assert_eq!(
        b.trigger.push_data(change(DataKind::Disclosure, "dart-1", t0)),
        DataPush::Queued
    );
    assert_eq!(b.trigger.push_data(change(DataKind::News, "news-2", t0)), DataPush::Queued);
    assert_eq!(b.trigger.discard_data(&[DataKind::News]), 2, "뉴스 둘을 버린다");
    assert_eq!(b.trigger.discard_data(&[DataKind::News]), 0, "두 번 버릴 것은 없다");
    b.clock.advance(Duration::from_secs(31));
    tick(&b).await;
    assert_eq!(woken_ids(&b), vec!["dart-1".to_string()]);
}

// 검증: F2 E14
#[tokio::test]
async fn 장_밖에_보관한_사건도_버려_장이_열려도_깨우지_않는다() {
    let night = Utc.with_ymd_and_hms(2026, 11, 15, 12, 0, 0).unwrap();
    let b = bench(night);
    b.trigger.push_data(change(DataKind::Macro, "macro-1", night));
    b.clock.advance(Duration::from_secs(31));
    tick(&b).await;
    assert!(woken_ids(&b).is_empty(), "장 밖이라 보관한다");
    assert_eq!(
        b.trigger
            .discard_data(&[DataKind::Disclosure, DataKind::Financials, DataKind::Macro]),
        1
    );
    b.clock.set(Utc.with_ymd_and_hms(2026, 11, 16, 0, 5, 0).unwrap());
    tick(&b).await;
    assert!(woken_ids(&b).is_empty(), "버린 사건으로는 깨우지 않는다");
    assert_eq!(
        b.trigger.push_data(change(DataKind::Macro, "macro-1", night)),
        DataPush::Duplicate,
        "버려도 중복 표는 그대로다(같은 사건으로 다시 깨우지 않는다)"
    );
}
