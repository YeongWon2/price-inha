//! 6차(2026-11-21 발표 자료 대조): 자료 변화 사건으로 깨우기와 지켜보는 종목의 시세 조건 목록.
//!
//! - 자료 변화: 중복 막기 `(source, id, revision)` · 출처마다 속도 제한 · 받지 않는 종류 ·
//!   묶어 보내기(조용한 시간 30초 · 최대 300초 · 상한 20) · 장 밖 보관 · 전체 정지 중 버림 ·
//!   같은 대상의 실행 중 · 오래 걸리는 깨우기 중의 늦은 시세 감시(N6).
//! - 시세 조건 목록: `watch_conditions` · `replace_conditions`.
//!
//! 원본(Hermes webhook)은 묶음의 마지막 사건 하나만 보낸다. 우리는 묶음의 사건을 모두(상한 20) 싣는다 —
//! 의도해서 다름(결정 2026-11-21 13번). 시각은 가짜 시계로만 움직인다.

use std::time::Duration;

use chrono::{DateTime, Utc};
use pi_market::{Clock, Market, Symbol};
use pi_trigger::{
    ConditionJob, ConditionKind, ConditionMemory, DEFAULT_WATCH_CHANGE_PERCENT, DataChange,
    DataKind, DataPush, Direction, Error, JobId, JobState, TickOutcome, TickReport, Trigger,
    TriggerConfig, TriggerEvent, TriggerStore, Wake, WakeReason, watch_conditions,
};
use rust_decimal::Decimal;

use crate::support::{Harness, condition, config, d, quote, secs, settled_tick, utc};

/// 국내 정규장 안(2026-11-16 10:00 KST).
fn in_session() -> DateTime<Utc> {
    utc(2026, 11, 16, 1, 0, 0)
}

/// 기다림의 상한(tokio 시간이 멈춰 있어 실제로는 잠들지 않는다).
const LIMIT: Duration = Duration::from_secs(5);

/// 뉴스 공시 시세 담당.
fn scout() -> pi_trigger::Target {
    pi_trigger::Target::Member("scout".to_string())
}

/// 국내 자료 변화 사건. 낸 시각 · 받은 시각은 `at`, 지문은 없다.
fn change(
    kind: DataKind,
    source: &str,
    id: &str,
    symbol: Option<&str>,
    at: DateTime<Utc>,
) -> DataChange {
    DataChange {
        kind,
        source: source.to_string(),
        id: id.to_string(),
        revision: None,
        market: Market::Kr,
        symbol: symbol.map(Symbol::new),
        title: format!("{id} 제목"),
        published_at: at,
        learned_at: at,
    }
}

/// 깨우기에 실린 자료 사건들(실린 순)과 보관 표시.
fn data_events(wake: &Wake) -> Vec<(DataChange, bool)> {
    wake.events
        .iter()
        .filter_map(|event| match event {
            TriggerEvent::Data { change, held } => Some((change.clone(), *held)),
            _ => None,
        })
        .collect()
}

/// 깨우기를 기다리지 않고 tick이 끝나야 한다.
async fn quick_tick(trigger: &Trigger) -> TickReport {
    tokio::time::timeout(LIMIT, trigger.tick())
        .await
        .expect("tick이 깨운 실행이 끝나기를 기다렸다(잠금을 쥐고 있다)")
        .unwrap()
}

fn now(h: &Harness) -> DateTime<Utc> {
    h.clock.now()
}

// 검증: F2, E14
#[test]
fn 자료_깨우기와_지켜보는_종목_조건의_기본값() {
    let config = TriggerConfig::new(vec![], vec![]);
    let data = config.data;
    assert_eq!(
        data.target,
        scout(),
        "F2 1: 뉴스 공시 시세 담당이 계기를 알아차린다"
    );
    for kind in [
        DataKind::News,
        DataKind::Disclosure,
        DataKind::Financials,
        DataKind::Macro,
    ] {
        assert!(data.enabled.contains(&kind), "{kind:?}는 기본으로 받는다");
    }
    assert_eq!(data.dedupe_for, secs(60 * 60));
    assert_eq!(data.quiet, secs(30));
    assert_eq!(data.max_wait, secs(300));
    assert_eq!(data.per_source_window, secs(60));
    assert_eq!(data.per_source_limit, 30);
    assert_eq!(data.batch_limit, 20);
    assert_eq!(DEFAULT_WATCH_CHANGE_PERCENT, d("3"));
}

// 검증: F2, E14
#[tokio::test]
async fn 같은_키는_한_시간_안에_중복이고_지문이_다르면_따로_받고_한_시간이_지나면_다시_받는다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));
    let first = DataChange {
        revision: Some("v1".to_string()),
        ..change(
            DataKind::Financials,
            "dart",
            "005930/2026Q3",
            Some("005930"),
            t0,
        )
    };
    let revised = DataChange {
        revision: Some("v2".to_string()),
        ..first.clone()
    };

    assert_eq!(trigger.push_data(first.clone()), DataPush::Queued);
    assert_eq!(trigger.push_data(first.clone()), DataPush::Duplicate);
    assert_eq!(trigger.push_data(revised.clone()), DataPush::Queued);

    h.clock.advance(secs(30));
    settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1, "같은 키는 깨우기 하나");
    let events: Vec<DataChange> = data_events(&wakes[0]).into_iter().map(|(c, _)| c).collect();
    assert_eq!(events, vec![first.clone(), revised]);

    // 첫 사건에서 한 시간이 지나면 같은 키도 다시 받는다.
    h.clock.set(t0 + chrono::Duration::seconds(3601));
    let again = DataChange {
        learned_at: now(&h),
        ..first
    };
    assert_eq!(trigger.push_data(again), DataPush::Queued);
}

// 검증: F2, E14
#[tokio::test]
async fn 출처마다_60초에_30개를_넘으면_속도_제한이고_61초_뒤에는_다시_받는다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));

    for n in 0..30 {
        let id = format!("dart-{n:02}");
        assert_eq!(
            trigger.push_data(change(
                DataKind::Disclosure,
                "dart",
                &id,
                Some("005930"),
                t0
            )),
            DataPush::Queued,
            "{id}"
        );
    }
    let limited = change(DataKind::Disclosure, "dart", "dart-30", Some("005930"), t0);
    assert_eq!(trigger.push_data(limited.clone()), DataPush::RateLimited);
    // 다른 출처는 따로 센다.
    assert_eq!(
        trigger.push_data(change(
            DataKind::News,
            "naver",
            "naver-00",
            Some("005930"),
            t0
        )),
        DataPush::Queued
    );

    // 61초 뒤: 속도 제한으로 넣지 않은 것은 중복 표에도 없어 다시 받는다.
    h.clock.advance(secs(61));
    assert_eq!(trigger.push_data(limited), DataPush::Queued);
    let at = now(&h);
    assert_eq!(
        trigger.push_data(change(
            DataKind::Disclosure,
            "dart",
            "dart-31",
            Some("005930"),
            at
        )),
        DataPush::Queued
    );
}

// 검증: F2, E14
#[tokio::test]
async fn 받지_않는_종류는_넣지_않는다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let mut cfg = config(vec![], vec![]);
    cfg.data.enabled = vec![DataKind::News];
    let trigger = h.trigger(cfg);

    let news = change(DataKind::News, "naver", "news-1", Some("005930"), t0);
    assert_eq!(
        trigger.push_data(change(
            DataKind::Disclosure,
            "dart",
            "d-1",
            Some("005930"),
            t0
        )),
        DataPush::Disabled
    );
    assert_eq!(
        trigger.push_data(change(
            DataKind::Macro,
            "fred",
            "DGS10/2026-11-20",
            None,
            t0
        )),
        DataPush::Disabled
    );
    assert_eq!(trigger.push_data(news.clone()), DataPush::Queued);

    h.clock.advance(secs(30));
    settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(data_events(&wakes[0]), vec![(news, false)]);
}

// 검증: F2, E14
#[tokio::test]
async fn 조용한_시간_29초에는_깨우지_않고_30초에_담당을_자료_이유로_깨운다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));
    let disclosure = change(
        DataKind::Disclosure,
        "dart",
        "20261116000123",
        Some("005930"),
        t0,
    );
    assert_eq!(trigger.push_data(disclosure.clone()), DataPush::Queued);

    settled_tick(&trigger).await;
    h.clock.advance(secs(29));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 0, "29초에는 아직 조용한 시간이다");

    h.clock.advance(secs(1));
    let report = settled_tick(&trigger).await;
    assert_eq!(report.data_dropped, 0);
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].target, scout());
    assert_eq!(wakes[0].reason, WakeReason::Data);
    assert_eq!(
        wakes[0].events,
        vec![TriggerEvent::Data {
            change: disclosure,
            held: false,
        }]
    );

    // 보낸 묶음은 다시 보내지 않는다.
    h.clock.advance(secs(60));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1);
}

// 검증: F2, E14
#[tokio::test]
async fn 계속_와도_첫_사건에서_300초면_깨우고_묶음은_상한_20개만_싣는다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));
    let mut dropped = 0;

    // 10초마다 하나씩 30개(0초 ~ 290초). 조용한 시간이 오지 않는다.
    for n in 0..30 {
        let at = now(&h);
        let id = format!("news-{n:02}");
        assert_eq!(
            trigger.push_data(change(DataKind::News, "naver", &id, Some("005930"), at)),
            DataPush::Queued
        );
        dropped += settled_tick(&trigger).await.data_dropped;
        assert_eq!(h.waker.count(), 0, "{}초에는 깨우지 않는다", n * 10);
        h.clock.advance(secs(10));
    }

    assert_eq!(now(&h), t0 + chrono::Duration::seconds(300));
    dropped += settled_tick(&trigger).await.data_dropped;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1, "첫 사건에서 300초면 깨운다");
    let ids: Vec<String> = data_events(&wakes[0])
        .into_iter()
        .map(|(c, _)| c.id)
        .collect();
    let expected: Vec<String> = (10..30).map(|n| format!("news-{n:02}")).collect();
    assert_eq!(ids, expected, "이른 순으로, 오래된 10개를 버리고 20개");
    assert_eq!(dropped, 10);
}

// 검증: F2, E14
#[tokio::test]
async fn 묶음이_21개면_가장_오래된_하나를_버리고_센다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));
    for n in 0..21 {
        let id = format!("d-{n:02}");
        let event = DataChange {
            published_at: t0 - chrono::Duration::seconds(60 - n),
            ..change(DataKind::Disclosure, "dart", &id, Some("005930"), t0)
        };
        assert_eq!(trigger.push_data(event), DataPush::Queued);
    }

    h.clock.advance(secs(30));
    let report = settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    let ids: Vec<String> = data_events(&wakes[0])
        .into_iter()
        .map(|(c, _)| c.id)
        .collect();
    let expected: Vec<String> = (1..21).map(|n| format!("d-{n:02}")).collect();
    assert_eq!(ids, expected);
    assert_eq!(report.data_dropped, 1);
}

// 검증: E18, F2, E14
#[tokio::test]
async fn 장_밖에_온_공시는_보관했다가_장_시작_뒤_첫_tick에_한_번_보관_표시로_넘긴다() {
    // 2026-11-16 08:00 KST: 정규장 전.
    let before = utc(2026, 11, 15, 23, 0, 0);
    let h = Harness::new(before);
    let trigger = h.trigger(config(vec![], vec![]));
    let disclosure = change(
        DataKind::Disclosure,
        "dart",
        "20261116000007",
        Some("005930"),
        before,
    );
    assert_eq!(trigger.push_data(disclosure.clone()), DataPush::Queued);

    settled_tick(&trigger).await;
    h.clock.advance(secs(30 * 60));
    settled_tick(&trigger).await;
    assert_eq!(
        h.waker.count(),
        0,
        "장 밖에는 조용한 시간이 지나도 깨우지 않는다"
    );

    h.clock.set(utc(2026, 11, 16, 0, 0, 10));
    settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].target, scout());
    assert_eq!(wakes[0].reason, WakeReason::Data);
    assert_eq!(data_events(&wakes[0]), vec![(disclosure, true)]);

    h.clock.advance(secs(60));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1, "한 번만 넘긴다");
}

// 검증: F2, E14
#[tokio::test]
async fn 전체_정지_중이면_모인_자료_사건을_깨우지_않고_버리고_센다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));
    assert_eq!(
        trigger.push_data(change(
            DataKind::Disclosure,
            "dart",
            "a",
            Some("005930"),
            t0
        )),
        DataPush::Queued
    );
    assert_eq!(
        trigger.push_data(change(DataKind::News, "naver", "b", Some("000660"), t0)),
        DataPush::Queued
    );

    h.clock.advance(secs(30));
    h.stop.set(true);
    let report = trigger.tick().await.unwrap();
    assert_eq!(report.outcome, TickOutcome::Stopped);
    assert!(report.wakes.is_empty());
    assert_eq!(report.data_dropped, 2);

    h.stop.set(false);
    h.clock.advance(secs(300));
    settled_tick(&trigger).await;
    assert_eq!(
        h.waker.count(),
        0,
        "버린 사건은 정지가 풀려도 깨우지 않는다"
    );
}

// 검증: F2, E14
#[tokio::test]
async fn 한_tick에_시세_조건과_자료_사건이_같은_대상이면_깨우기_하나에_묶이고_이유는_시세() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let samsung = ConditionJob {
        target: scout(),
        ..condition("samsung-100", "005930", ConditionKind::Above(d("100")))
    };
    let trigger = h.trigger(config(vec![], vec![samsung]));
    let disclosure = change(
        DataKind::Disclosure,
        "dart",
        "20261116000123",
        Some("005930"),
        t0,
    );
    assert_eq!(trigger.push_data(disclosure.clone()), DataPush::Queued);

    h.clock.advance(secs(30));
    h.market.set_quote(quote("005930", "100", now(&h)));
    settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(wakes[0].target, scout());
    assert_eq!(wakes[0].reason, WakeReason::Price);
    assert_eq!(wakes[0].events.len(), 2);
    assert!(
        wakes[0]
            .events
            .iter()
            .any(|event| matches!(event, TriggerEvent::Price { job, .. } if *job == JobId::new("samsung-100")))
    );
    assert_eq!(data_events(&wakes[0]), vec![(disclosure, false)]);
}

// 검증: F2, E14
#[tokio::test(start_paused = true)]
async fn 같은_대상의_깨우기가_실행_중이면_보내지_않고_묶음에_남겨_다음_tick에_보낸다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));
    h.waker.hold(true);
    h.waker.hold_target(Some(scout()));

    let first = change(DataKind::Disclosure, "dart", "first", Some("005930"), t0);
    assert_eq!(trigger.push_data(first.clone()), DataPush::Queued);
    h.clock.advance(secs(30));
    let report = quick_tick(&trigger).await;
    assert_eq!(report.wakes.len(), 1);
    tokio::time::timeout(LIMIT, h.waker.entered())
        .await
        .expect("자료 깨우기가 띄워져야 한다");

    let second = change(
        DataKind::Disclosure,
        "dart",
        "second",
        Some("005930"),
        now(&h),
    );
    assert_eq!(trigger.push_data(second.clone()), DataPush::Queued);
    h.clock.advance(secs(30));
    let report = quick_tick(&trigger).await;
    assert_eq!(report.outcome, TickOutcome::Ran);
    assert!(
        report.wakes.iter().all(|wake| wake.target != scout()),
        "같은 대상이 실행 중이면 보내지 않는다"
    );
    assert_eq!(report.data_dropped, 0, "버리지 않고 묶음에 남긴다");

    h.waker.hold(false);
    h.waker.release();
    tokio::time::timeout(LIMIT, trigger.settle())
        .await
        .expect("풀어 준 깨우기는 끝나야 한다")
        .unwrap();
    h.clock.advance(secs(1));
    quick_tick(&trigger).await;
    tokio::time::timeout(LIMIT, trigger.settle())
        .await
        .expect("깨우기가 끝나야 한다")
        .unwrap();
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 2);
    assert_eq!(data_events(&wakes[0]), vec![(first, false)]);
    assert_eq!(data_events(&wakes[1]), vec![(second, false)]);
}

// 검증: N6, E14, F2
#[tokio::test(start_paused = true)]
async fn 자료_깨우기가_오래_걸리는_동안에도_늦은_시세_감시가_돈다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let samsung = condition("samsung-100", "005930", ConditionKind::Above(d("100")));
    let trigger = h.trigger(config(vec![], vec![samsung]));
    h.waker.hold(true);
    h.waker.hold_target(Some(scout()));

    h.market.set_quote(quote("005930", "50", t0));
    assert_eq!(
        trigger.push_data(change(DataKind::News, "naver", "n-1", Some("005930"), t0)),
        DataPush::Queued
    );
    quick_tick(&trigger).await;

    h.clock.advance(secs(30));
    let t1 = now(&h);
    h.market.set_quote(quote("005930", "50", t1));
    let report = quick_tick(&trigger).await;
    assert_eq!(report.wakes.len(), 1);
    assert_eq!(report.wakes[0].target, scout());
    tokio::time::timeout(LIMIT, h.waker.entered())
        .await
        .expect("자료 깨우기가 띄워져야 한다");

    // 시세가 31초째 오지 않는다: 자료 깨우기가 끝나지 않았어도 팀을 이상으로 깨운다.
    h.clock.set(t1 + chrono::Duration::seconds(31));
    let report = quick_tick(&trigger).await;
    assert_eq!(report.outcome, TickOutcome::Ran, "바쁨이 아니어야 한다");
    let anomaly = report
        .wakes
        .iter()
        .find(|wake| wake.target == pi_trigger::Target::Team)
        .expect("늦은 시세로 팀을 깨워야 한다");
    assert_eq!(anomaly.reason, WakeReason::Anomaly);
    assert_eq!(
        anomaly.events,
        vec![TriggerEvent::StaleQuote {
            market: Market::Kr,
            symbol: Symbol::new("005930"),
            last_received: Some(t1),
        }]
    );

    h.waker.release();
    tokio::time::timeout(LIMIT, trigger.settle())
        .await
        .expect("풀어 준 깨우기는 끝나야 한다")
        .unwrap();
    assert_eq!(trigger.in_flight(), 0);
}

// 검증: F2
#[test]
fn 지켜보는_종목마다_변동률_조건_하나를_만들고_기준가_0_이하는_뺀다() {
    let watched = vec![
        (Symbol::new("005930"), d("70000")),
        (Symbol::new("000660"), d("0")),
        (Symbol::new("035420"), d("-5")),
        (Symbol::new("035720"), d("50000")),
    ];
    let jobs = watch_conditions(Market::Kr, &watched, DEFAULT_WATCH_CHANGE_PERCENT, scout());
    assert_eq!(
        jobs,
        vec![
            ConditionJob {
                id: JobId::new("price:kr:005930:change"),
                market: Market::Kr,
                symbol: Symbol::new("005930"),
                kind: ConditionKind::Change {
                    reference: d("70000"),
                    percent: d("3"),
                },
                target: scout(),
            },
            ConditionJob {
                id: JobId::new("price:kr:035720:change"),
                market: Market::Kr,
                symbol: Symbol::new("035720"),
                kind: ConditionKind::Change {
                    reference: d("50000"),
                    percent: d("3"),
                },
                target: scout(),
            },
        ]
    );

    let us = watch_conditions(
        Market::Us,
        &[(Symbol::new("AAPL"), d("230.5"))],
        d("5"),
        pi_trigger::Target::Team,
    );
    assert_eq!(us.len(), 1);
    assert_eq!(us[0].id, JobId::new("price:us:AAPL:change"));
    assert_eq!(us[0].market, Market::Us);
    assert_eq!(
        us[0].kind,
        ConditionKind::Change {
            reference: d("230.5"),
            percent: d("5"),
        }
    );
    assert!(watch_conditions(Market::Kr, &[], d("3"), scout()).is_empty());
}

// 검증: F2
#[tokio::test]
async fn 조건_목록_바꾸기가_잘못되면_거부하고_지금_목록을_그대로_둔다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let samsung = condition("samsung-100", "005930", ConditionKind::Above(d("100")));
    let trigger = h.trigger(config(vec![], vec![samsung]));

    let twice = condition("x", "000660", ConditionKind::Above(d("10")));
    let result = trigger.replace_conditions(vec![twice.clone(), twice]).await;
    assert!(
        matches!(&result, Err(Error::DuplicateJob(id)) if *id == JobId::new("x")),
        "{result:?}"
    );

    let us = ConditionJob {
        market: Market::Us,
        ..condition("us", "AAPL", ConditionKind::Above(d("10")))
    };
    let result = trigger.replace_conditions(vec![us]).await;
    assert!(
        matches!(&result, Err(Error::MissingMarket(_))),
        "{result:?}"
    );

    let zero = condition(
        "zero",
        "000660",
        ConditionKind::Change {
            reference: d("0"),
            percent: d("3"),
        },
    );
    let result = trigger.replace_conditions(vec![zero]).await;
    assert!(
        matches!(&result, Err(Error::InvalidCondition(_))),
        "{result:?}"
    );

    // 지금 목록(삼성 100 이상)이 그대로 돈다.
    h.market.set_quote(quote("005930", "100", t0));
    h.market.set_quote(quote("000660", "100", t0));
    settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert!(matches!(
        &wakes[0].events[..],
        [TriggerEvent::Price { job, .. }] if *job == JobId::new("samsung-100")
    ));
}

// 검증: F2
#[tokio::test]
async fn 같은_조건은_알린_상태를_유지하고_기준가가_바뀐_조건은_다시_건다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let samsung = |reference: &str| {
        watch_conditions(
            Market::Kr,
            &[(Symbol::new("005930"), d(reference))],
            d("3"),
            scout(),
        )
    };
    let id = "price:kr:005930:change";
    let trigger = h.trigger(config(vec![], samsung("100")));

    h.market.set_quote(quote("005930", "103", t0));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1);
    let fired = Some(ConditionMemory::Fired {
        notified: d("103"),
        direction: Direction::Up,
    });
    assert_eq!(h.store.job(id).await.condition, fired);

    // 같은 id · 같은 조건: 알린 상태 그대로, 다시 알리지 않는다.
    trigger.replace_conditions(samsung("100")).await.unwrap();
    assert_eq!(h.store.job(id).await.condition, fired);
    h.clock.advance(secs(10));
    h.market.set_quote(quote("005930", "104", now(&h)));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1);

    // 다음 날 기준가가 103으로 바뀌었다: 다시 건다. 107은 103에서 3% 넘게 올랐다.
    trigger.replace_conditions(samsung("103")).await.unwrap();
    let memory = h.store.job(id).await.condition;
    assert!(
        matches!(memory, None | Some(ConditionMemory::Armed)),
        "{memory:?}"
    );
    h.clock.advance(secs(10));
    h.market.set_quote(quote("005930", "107", now(&h)));
    settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 2);
    assert!(matches!(
        &wakes[1].events[..],
        [TriggerEvent::Price { condition: ConditionKind::Change { reference, .. }, price, .. }]
            if *reference == d("103") && *price == d("107")
    ));
}

// 검증: F2
#[tokio::test]
async fn 빠진_조건은_더_보지_않는다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let samsung = condition("samsung-100", "005930", ConditionKind::Above(d("100")));
    let trigger = h.trigger(config(vec![], vec![samsung]));

    trigger.replace_conditions(vec![]).await.unwrap();
    h.market.set_quote(quote("005930", "150", t0));
    settled_tick(&trigger).await;
    h.clock.advance(secs(60));
    h.market.set_quote(quote("005930", "160", now(&h)));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 0);
}

// ---- 6차 보강(검토 되돌림 2026-11-21) ----
//
// 1. 알린 기억(`Fired{알린 값, 방향}`)은 그 값이 지금 조건의 문턱을 그 방향으로 넘은 값일 때만 쓴다.
//    성립하지 않으면 걸린 것(`Armed`)으로 본다. 저장소에 무엇이 적혀 있는지는 묻지 않고 알리는지만 본다.
// 2. 상한으로 버린 자료 사건은 tick 보고의 `data_dropped`에 보인다.
// 3. 실패한 자료 깨우기의 사건은 버린 것으로 센다.

/// 지켜보는 삼성전자 조건의 id([`watch_conditions`]가 만드는 모양).
const WATCH_ID: &str = "price:kr:005930:change";

/// 삼성전자를 그 기준가 · 3%로 지켜보는 조건 목록(대상: 담당).
fn samsung_watch(reference: &str) -> Vec<ConditionJob> {
    watch_conditions(
        Market::Kr,
        &[(Symbol::new("005930"), d(reference))],
        d("3"),
        scout(),
    )
}

/// 저장소에 그 조건의 알린 기억을 미리 둔다(지난 실행이 남긴 것처럼).
async fn remember_fired(h: &Harness, notified: &str, direction: Direction, at: DateTime<Utc>) {
    let mut state = JobState::new(JobId::new(WATCH_ID), Some(at));
    state.condition = Some(ConditionMemory::Fired {
        notified: d(notified),
        direction,
    });
    h.store.inner_save(&state).await;
}

/// 깨우기에 실린 변동률 조건의 시세 사건들: (기준가, 가격, 방향).
fn change_events(wake: &Wake) -> Vec<(Decimal, Decimal, Direction)> {
    wake.events
        .iter()
        .filter_map(|event| match event {
            TriggerEvent::Price {
                condition: ConditionKind::Change { reference, .. },
                price,
                direction,
                ..
            } => Some((*reference, *price, *direction)),
            _ => None,
        })
        .collect()
}

/// 10초 뒤의 새 시세를 넣고 tick 한 번(띄운 깨우기가 끝날 때까지).
async fn tick_at_price(h: &Harness, trigger: &Trigger, price: &str) -> TickReport {
    h.clock.advance(secs(10));
    h.market.set_quote(quote("005930", price, now(h)));
    settled_tick(trigger).await
}

// 검증: F2
#[tokio::test(start_paused = true)]
async fn 깨우기가_실행_중일_때_기준가가_바뀌면_옛_깨우기가_끝난_뒤에도_새_문턱에서_다시_알린다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], samsung_watch("100")));
    h.waker.hold(true);
    h.waker.hold_target(Some(scout()));

    // 기준가 100에서 3% 오른 103: 알리는 깨우기를 붙잡아 둔다.
    h.market.set_quote(quote("005930", "103", t0));
    let report = quick_tick(&trigger).await;
    assert_eq!(report.wakes.len(), 1);
    tokio::time::timeout(LIMIT, h.waker.entered())
        .await
        .expect("시세 조건 깨우기가 띄워져야 한다");

    // 깨우기가 도는 동안 기준가가 103으로 바뀐다(새 문턱: 106.09 이상 · 99.91 이하).
    tokio::time::timeout(LIMIT, trigger.replace_conditions(samsung_watch("103")))
        .await
        .expect("실행 중인 깨우기를 기다리지 않고 목록을 바꾼다")
        .unwrap();

    // 옛 깨우기가 성공으로 끝난다(옛 조건의 알린 값 103을 적으려 한다).
    h.waker.hold(false);
    h.waker.release();
    tokio::time::timeout(LIMIT, trigger.settle())
        .await
        .expect("풀어 준 깨우기는 끝나야 한다")
        .unwrap();
    assert_eq!(h.waker.count(), 1);

    tick_at_price(&h, &trigger, "105").await;
    assert_eq!(h.waker.count(), 1, "105는 새 문턱(106.09) 아래다");

    tick_at_price(&h, &trigger, "106.09").await;
    let wakes = h.waker.wakes();
    assert_eq!(
        wakes.len(),
        2,
        "옛 조건의 알린 값 103은 새 문턱을 넘지 않았으니 새 문턱에서 다시 알린다"
    );
    assert_eq!(
        change_events(&wakes[1]),
        vec![(d("103"), d("106.09"), Direction::Up)]
    );
}

// 검증: F2
#[tokio::test]
async fn 알린_종목을_목록에서_뺐다가_다른_기준가로_다시_넣으면_새_문턱에서_알린다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], samsung_watch("100")));

    h.market.set_quote(quote("005930", "103", t0));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 1);
    assert_eq!(
        h.store.job(WATCH_ID).await.condition,
        Some(ConditionMemory::Fired {
            notified: d("103"),
            direction: Direction::Up,
        })
    );

    // 목록에서 뺀다(저장소에는 알린 기억이 남는다). 그동안은 보지 않는다.
    trigger.replace_conditions(vec![]).await.unwrap();
    tick_at_price(&h, &trigger, "110").await;
    assert_eq!(h.waker.count(), 1);

    // 다른 기준가(103)로 다시 넣는다.
    trigger
        .replace_conditions(samsung_watch("103"))
        .await
        .unwrap();
    tick_at_price(&h, &trigger, "105").await;
    assert_eq!(h.waker.count(), 1, "105는 새 문턱(106.09) 아래다");

    tick_at_price(&h, &trigger, "106.09").await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 2, "남은 옛 알린 기억이 새 조건을 막지 않는다");
    assert_eq!(
        change_events(&wakes[1]),
        vec![(d("103"), d("106.09"), Direction::Up)]
    );
    assert_eq!(
        h.store.job(WATCH_ID).await.condition,
        Some(ConditionMemory::Fired {
            notified: d("106.09"),
            direction: Direction::Up,
        })
    );
}

// 검증: F2, E16
#[tokio::test]
async fn 저장소에_알린_기억이_있어도_다른_기준가로_켜면_새_문턱에서_알린다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    // 어제: 기준가 100에서 103으로 올라 알렸다. 오늘은 기준가 103으로 다시 켠다.
    remember_fired(&h, "103", Direction::Up, t0).await;
    let trigger = h.trigger(config(vec![], samsung_watch("103")));
    trigger.start().await.unwrap();

    h.market.set_quote(quote("005930", "105", t0));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 0, "105는 새 문턱(106.09) 아래다");

    tick_at_price(&h, &trigger, "106.09").await;
    let wakes = h.waker.wakes();
    assert_eq!(
        wakes.len(),
        1,
        "알린 값 103은 지금 조건의 문턱(106.09)을 넘은 값이 아니다"
    );
    assert_eq!(wakes[0].target, scout());
    assert_eq!(
        change_events(&wakes[0]),
        vec![(d("103"), d("106.09"), Direction::Up)]
    );

    // 새로 알린 뒤에는 결정 21번 그대로: 되돌아오기 전에는 다시 알리지 않는다.
    tick_at_price(&h, &trigger, "107").await;
    assert_eq!(h.waker.count(), 1);
}

// 검증: F2, E16
#[tokio::test]
async fn 내린_쪽으로_알린_기억도_지금_문턱을_넘은_값이_아니면_새_문턱에서_알린다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    // 어제: 기준가 100에서 97로 내려 알렸다. 오늘 기준가는 95(내린 쪽 문턱: 92.15 이하).
    remember_fired(&h, "97", Direction::Down, t0).await;
    let trigger = h.trigger(config(vec![], samsung_watch("95")));
    trigger.start().await.unwrap();

    h.market.set_quote(quote("005930", "93", t0));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 0, "93은 새 문턱(92.15) 위다");

    tick_at_price(&h, &trigger, "92.15").await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(
        change_events(&wakes[0]),
        vec![(d("95"), d("92.15"), Direction::Down)]
    );
}

// 검증: F2, E16
#[tokio::test]
async fn 기준가가_같으면_알린_기억을_유지해_되돌아온_뒤에만_다시_알린다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    // 알린 값 103은 기준가 100 · 3%의 문턱(103 이상)에 닿은 값이다(경계 포함).
    remember_fired(&h, "103", Direction::Up, t0).await;
    let trigger = h.trigger(config(vec![], samsung_watch("100")));
    trigger.start().await.unwrap();

    // 같은 기준가로 다시 켰다: 다시 알리지 않는다.
    h.market.set_quote(quote("005930", "104", t0));
    settled_tick(&trigger).await;
    assert_eq!(h.waker.count(), 0, "같은 기준가면 알린 기억이 그대로다");

    // 목록에서 뺐다가 같은 기준가로 다시 넣어도 다시 알리지 않는다.
    trigger.replace_conditions(vec![]).await.unwrap();
    trigger
        .replace_conditions(samsung_watch("100"))
        .await
        .unwrap();
    tick_at_price(&h, &trigger, "105").await;
    assert_eq!(h.waker.count(), 0);

    // 알린 값에서 0.5% 되돌아오기 전(102.49)에는 다시 걸리지 않는다.
    tick_at_price(&h, &trigger, "102.49").await;
    tick_at_price(&h, &trigger, "104").await;
    assert_eq!(h.waker.count(), 0);

    // 0.5% 되돌아온 뒤(102.485 이하)에만 다시 걸리고, 그 tick에는 알리지 않는다(결정 2026-11-16 21번).
    tick_at_price(&h, &trigger, "102.485").await;
    assert_eq!(h.waker.count(), 0);
    tick_at_price(&h, &trigger, "103").await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    assert_eq!(
        change_events(&wakes[0]),
        vec![(d("100"), d("103"), Direction::Up)]
    );
}

// 검증: E18, F2, E14
#[tokio::test]
async fn 장_밖_보관이_21개면_가장_오래된_하나를_버린_것이_보고에_보인다() {
    // 2026-11-16 08:00 KST: 정규장 전.
    let before = utc(2026, 11, 15, 23, 0, 0);
    let h = Harness::new(before);
    let trigger = h.trigger(config(vec![], vec![]));
    // 종목이 모두 달라 묶음마다 하나씩이다(묶음 상한이 아니라 보관 상한에 걸린다).
    for n in 0..21 {
        let id = format!("d-{n:02}");
        let symbol = format!("{:06}", 100 + n);
        let event = DataChange {
            published_at: before - chrono::Duration::seconds(60 - n),
            ..change(DataKind::Disclosure, "dart", &id, Some(&symbol), before)
        };
        assert_eq!(trigger.push_data(event), DataPush::Queued);
    }

    h.clock.advance(secs(30 * 60));
    let mut dropped = settled_tick(&trigger).await.data_dropped;
    assert_eq!(h.waker.count(), 0, "장 밖에는 깨우지 않는다");

    h.clock.set(utc(2026, 11, 16, 0, 0, 10));
    dropped += settled_tick(&trigger).await.data_dropped;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1);
    let events = data_events(&wakes[0]);
    assert!(events.iter().all(|(_, held)| *held), "모두 보관 표시");
    let ids: Vec<String> = events.into_iter().map(|(c, _)| c.id).collect();
    let expected: Vec<String> = (1..21).map(|n| format!("d-{n:02}")).collect();
    assert_eq!(ids, expected, "이른 순으로, 가장 오래된 하나를 버리고 20개");
    assert_eq!(
        dropped, 1,
        "보관 상한으로 버린 수가 tick 보고에 보여야 한다"
    );

    h.clock.advance(secs(60));
    let report = settled_tick(&trigger).await;
    assert_eq!(report.data_dropped, 0, "버린 수는 한 번만 센다");
    assert_eq!(h.waker.count(), 1);
}

// 검증: F2, E14
#[tokio::test]
async fn 여러_종목_묶음이_같은_tick에_익어_합이_20을_넘으면_넘는_수가_보고에_보인다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));
    // 세 종목에 8개씩 24개. 묶음 하나하나는 상한(20) 아래다.
    let symbols = ["005930", "000660", "035420"];
    for n in 0..24 {
        let id = format!("n-{n:02}");
        let event = DataChange {
            published_at: t0 - chrono::Duration::seconds(100 - n),
            ..change(
                DataKind::News,
                "naver",
                &id,
                Some(symbols[usize::try_from(n).unwrap() % 3]),
                t0,
            )
        };
        assert_eq!(trigger.push_data(event), DataPush::Queued);
    }

    h.clock.advance(secs(30));
    let report = settled_tick(&trigger).await;
    let wakes = h.waker.wakes();
    assert_eq!(wakes.len(), 1, "같은 대상이라 깨우기 하나");
    let ids: Vec<String> = data_events(&wakes[0])
        .into_iter()
        .map(|(c, _)| c.id)
        .collect();
    let expected: Vec<String> = (4..24).map(|n| format!("n-{n:02}")).collect();
    assert_eq!(ids, expected, "이른 순으로, 오래된 4개를 버리고 20개");
    assert_eq!(
        report.data_dropped, 4,
        "한 깨우기 상한으로 버린 수가 tick 보고에 보여야 한다"
    );

    h.clock.advance(secs(60));
    let report = settled_tick(&trigger).await;
    assert_eq!(report.data_dropped, 0);
    assert_eq!(h.waker.count(), 1);
}

// 검증: F2, E14
#[tokio::test]
async fn 자료_깨우기가_실패하면_그_사건들을_버린_것으로_세어_보고에_보인다() {
    let t0 = in_session();
    let h = Harness::new(t0);
    let trigger = h.trigger(config(vec![], vec![]));

    // 성공한 자료 깨우기는 버린 것이 아니다.
    assert_eq!(
        trigger.push_data(change(DataKind::News, "naver", "ok", Some("005930"), t0)),
        DataPush::Queued
    );
    h.clock.advance(secs(30));
    let mut succeeded = settled_tick(&trigger).await.data_dropped;
    h.clock.advance(secs(1));
    succeeded += settled_tick(&trigger).await.data_dropped;
    assert_eq!(h.waker.count(), 1);
    assert_eq!(succeeded, 0);

    // 깨운 실행이 실패한다.
    h.waker.fail(true);
    for (id, symbol) in [("a", "005930"), ("b", "000660")] {
        let at = now(&h);
        assert_eq!(
            trigger.push_data(change(DataKind::Disclosure, "dart", id, Some(symbol), at)),
            DataPush::Queued
        );
    }
    h.clock.advance(secs(30));
    let launched = settled_tick(&trigger).await;
    assert_eq!(launched.wakes.len(), 1);
    assert_eq!(data_events(&launched.wakes[0]).len(), 2);
    assert_eq!(h.waker.count(), 2);

    // 띄운 tick의 보고는 실패를 아직 모른다. 다음 tick 보고까지에는 잡혀야 한다.
    h.waker.fail(false);
    h.clock.advance(secs(1));
    let next = settled_tick(&trigger).await;
    assert_eq!(
        launched.data_dropped + next.data_dropped,
        2,
        "실패한 자료 깨우기의 사건 2개가 버린 수로 보여야 한다"
    );

    // 버린 사건은 다시 보내지 않고, 버린 수도 한 번만 센다. 자료 사건에는 영수증을 열지 않는다.
    h.clock.advance(secs(300));
    let later = settled_tick(&trigger).await;
    assert_eq!(later.data_dropped, 0);
    assert_eq!(h.waker.count(), 2);
    assert!(h.store.inner.receipts().await.unwrap().is_empty());
}
