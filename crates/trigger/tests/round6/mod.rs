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
    DataKind, DataPush, Direction, Error, JobId, TickOutcome, TickReport, Trigger, TriggerConfig,
    TriggerEvent, Wake, WakeReason, watch_conditions,
};

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
