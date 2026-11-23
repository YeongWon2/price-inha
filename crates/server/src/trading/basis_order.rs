//! (14차 2번) 장부 셈 둘이 동시에 최고 평가액을 올려 저장 순서가 뒤집혀도, 다시 켠 뒤 최고 평가액은 둘 가운데
//! 높은 값이다.
//!
//! 순서를 결정적으로 만든다: 낮은 최고를 담은 첫 셈이 저장 바로 앞에서 멈춰 서고(시험용 자리,
//! [`before_save`]), 그 사이에 더 높은 최고를 담은 둘째 셈이 끝까지 저장한다. 그 뒤 첫 셈을 풀어 주면
//! 낮은 기록이 나중에 저장되려 한다. 저장을 줄 세우는 구현이면 둘째 셈이 첫째를 기다리므로(풀어 주기 전에는
//! 끝나지 않음) 풀어 준 뒤에 낮은 쪽이 먼저, 높은 쪽이 나중에 저장된다. 어느 쪽이든 다시 켜서 되읽는다.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use chrono::NaiveDate;
use pi_market::Currency;
use rust_decimal::Decimal;

use super::Trading;
use super::books_race::{CASH, PRICE, fill, kst, quote, start};
use pi_paper::Side;

/// 저장 앞에서 첫 셈을 세워 두는 자리.
pub(super) struct Gate {
    taken: AtomicBool,
    held: AtomicBool,
    released: Mutex<bool>,
    wake: Condvar,
}

static GATES: Mutex<Vec<(usize, Arc<Gate>)>> = Mutex::new(Vec::new());

fn key(trading: &Trading) -> usize {
    std::ptr::from_ref(trading) as usize
}

fn arm(trading: &Trading) -> Arc<Gate> {
    let gate = Arc::new(Gate {
        taken: AtomicBool::new(false),
        held: AtomicBool::new(false),
        released: Mutex::new(false),
        wake: Condvar::new(),
    });
    GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((key(trading), Arc::clone(&gate)));
    gate
}

fn release(gate: &Gate) {
    *gate
        .released
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    gate.wake.notify_all();
}

/// `save_basis`가 저장 바로 앞에서 부르는 시험용 자리. 열어 둔 자리가 있으면 처음 온 한 번만 풀어 줄 때까지 세운다.
pub(super) fn before_save(trading: &Trading) {
    let gate = GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .find(|(owner, _)| *owner == key(trading))
        .map(|(_, gate)| Arc::clone(gate));
    let Some(gate) = gate else { return };
    if gate.taken.swap(true, Ordering::SeqCst) {
        return;
    }
    gate.held.store(true, Ordering::SeqCst);
    let released = gate
        .released
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // 풀어 주지 않아도 영영 서 있지 않게 한다(저장을 줄 세운 구현이면 풀어 주기를 둘째 셈이 막지 않는다).
    let _ = gate
        .wake
        .wait_timeout_while(released, Duration::from_secs(10), |done| !*done);
}

async fn until(what: &str, mut ready: impl FnMut() -> bool) {
    for _ in 0..500 {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what}");
}

fn priced(at: chrono::DateTime<chrono::Utc>, price: i64) -> pi_market::Quote {
    let mut q = quote(at);
    q.price = Decimal::from(price);
    q
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 최고_평가액을_동시에_올린_두_셈의_저장_순서가_뒤집혀도_다시_켠_최고는_높은_값이다() {
    let dir = tempfile::tempdir().unwrap();
    let at = kst(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(), 10, 0);
    let app = start(dir.path(), at).await;
    let trading = app.trading();
    // 평균 단가 = 시세로 사 두면 원화 평가액은 시세에 따라 처음 현금 위로 오른다.
    trading.remember(&quote(at));
    fill(&trading, Side::Buy, at + chrono::Duration::milliseconds(2)).await;
    trading.books(at).await.expect("처음 셈");

    // 첫 셈: 시세 110,000(평가액 +100,000)을 읽고 낮은 최고를 담은 기록을 저장 앞에서 멈춘다.
    trading.remember(&priced(at, PRICE + 10_000));
    let gate = arm(&trading);
    let first = {
        let trading = Arc::clone(&trading);
        tokio::spawn(async move { trading.books(at).await })
    };
    until("첫 셈이 저장 앞에 서야 한다", || {
        gate.held.load(Ordering::SeqCst)
    })
    .await;

    // 둘째 셈: 시세 120,000(평가액 +200,000)을 읽고 더 높은 최고를 담아 저장하려 한다.
    trading.remember(&priced(at, PRICE + 20_000));
    let second = {
        let trading = Arc::clone(&trading);
        tokio::spawn(async move { trading.books(at).await })
    };
    // 둘째가 끝나기를 잠깐 기다린다. 저장을 줄 세운 구현이면 첫째를 기다리느라 끝나지 않는다.
    for _ in 0..50 {
        if second.is_finished() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    release(&gate);
    first.await.unwrap().expect("첫 셈");
    second.await.unwrap().expect("둘째 셈");
    let higher = Decimal::from(CASH) + Decimal::from(200_000);

    tokio::time::timeout(Duration::from_secs(10), app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");

    // 다시 켜서(저장소에서 되읽는 길) 그 통화의 최고 평가액을 본다.
    let app = start(dir.path(), at).await;
    let books = app.trading().books(at).await.expect("다시 켠 장부");
    let krw = books
        .iter()
        .find(|book| book.currency == Currency::Krw)
        .expect("원화 장부");
    assert_eq!(
        krw.peak_equity, higher,
        "다시 켠 뒤 최고 평가액은 둘 가운데 높은 값이어야 한다(낮은 기록이 나중에 저장돼 되살아남)"
    );
    tokio::time::timeout(Duration::from_secs(10), app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}
