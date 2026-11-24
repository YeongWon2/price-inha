//! 21차 단위 시험(돈 · 안전): 공개 길로 만들 수 없는 두 가지.
//!
//! - 2번: 최고 평가액은 저장된 **뒤에만** "남김"으로 친다. 저장을 막는 트리거를 저장소 파일에 달아 저장이 한 번
//!   실패하게 하고, 그 뒤 다시 셈하면 같은 값을 다시 남겨 다시 켠 최고 평가액이 낮게 되살아나지 않는다.
//! - 3번 뒤: 이상 감지 멈춤 중에는 비정상 바퀴를 세지 않는다(풀린 뒤 새로 센다). 자료 이상(자료 받기 연속 실패)을
//!   만들어 두면 시세 흐름의 어느 걸음이든 비정상이라, 바퀴 수만 시험이 정한 `after_feed` 호출로 센다
//!   (시세 흐름은 한 시간에 한 번 받는 조용한 시계라 처음 한 번 뒤에는 끼어들지 않는다).

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use pi_market::{Currency, Quote, Symbol};
use pi_paper::Side;
use pi_store::{TeamCause, TeamChange, TeamState};
use rust_decimal::Decimal;

use super::base::millis;
use super::books_race::{CASH, PRICE, fill, kst, quote, start, start_quiet};
use crate::DayPlan;

fn monday_at(hour: u32, minute: u32) -> DateTime<Utc> {
    kst(NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(), hour, minute)
}

fn priced(at: DateTime<Utc>, price: i64) -> Quote {
    let mut q = quote(at);
    q.price = Decimal::from(price);
    q
}

/// 저장소 파일에 직접 SQL을 낸다(다른 연결이 쓰는 중이어도 잠깐 기다린다).
fn sql(home: &std::path::Path, statements: &str) {
    let conn = rusqlite::Connection::open(home.join(crate::DB_FILE)).unwrap();
    conn.busy_timeout(Duration::from_secs(5)).unwrap();
    conn.execute_batch(statements).unwrap();
}

// 검증: P4 V6 E4
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 최고_평가액_저장이_한_번_실패해도_다음_셈이_같은_값을_다시_남겨_다시_켠_최고가_낮지_않다()
{
    let dir = tempfile::tempdir().unwrap();
    let at = monday_at(10, 0);
    let app = start(dir.path(), at).await;
    let trading = app.trading();
    // 평균 단가 = 시세로 사 두면 원화 평가액은 시세에 따라 처음 현금 위로 오른다.
    trading.remember(&quote(at));
    fill(&trading, Side::Buy, at + chrono::Duration::milliseconds(2)).await;
    trading.books(at).await.expect("처음 셈");

    // 최고 평가액을 남기는 저장을 막고, 시세를 올려 최고를 올린다(평가액 +100,000).
    sql(
        dir.path(),
        "CREATE TRIGGER block_basis BEFORE INSERT ON events WHEN NEW.kind = 'loss_basis' \
         BEGIN SELECT RAISE(ABORT, '시험: 저장 실패'); END;",
    );
    trading.remember(&priced(at, PRICE + 10_000));
    assert!(
        trading.books(at).await.is_err(),
        "저장이 실패하면 셈도 실패로 알린다"
    );

    // 막은 것을 풀고 다음 tick을 흉내 낸다(시세 · 평가액은 그대로).
    sql(dir.path(), "DROP TRIGGER block_basis;");
    trading.books(at).await.expect("다음 셈");
    tokio::time::timeout(Duration::from_secs(10), app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");

    // 다시 켜서(저장소에서 되읽는 길) 최고 평가액을 본다.
    let app = start(dir.path(), at).await;
    let books = app.trading().books(at).await.expect("다시 켠 장부");
    let krw = books
        .iter()
        .find(|book| book.currency == Currency::Krw)
        .expect("원화 장부");
    assert_eq!(
        krw.peak_equity,
        Decimal::from(CASH) + Decimal::from(100_000),
        "저장이 실패했어도 다음 셈이 같은 값을 다시 남겨, 다시 켠 최고 평가액이 낮게 되살아나지 않는다"
    );
    tokio::time::timeout(Duration::from_secs(10), app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}

// 검증: N6 V11 P11
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 이상_감지_멈춤_중에는_비정상_바퀴를_세지_않아_풀린_뒤_다시_두_바퀴를_본다() {
    let dir = tempfile::tempdir().unwrap();
    let at = monday_at(10, 0);
    let app = start_quiet(dir.path(), at).await;
    let trading = app.trading();
    let store = app.store();
    // 시세 흐름의 처음 한 번이 끝나기를 기다린다(그 뒤에는 한 시간 동안 끼어들지 않는다).
    for _ in 0..200 {
        if trading.last_quote("005930").is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        trading.decide_day(DayPlan::Trade).unwrap(),
        TeamState::Operating,
        "오늘 매매하기로 하면 운용 중이다"
    );

    // 자료 받기가 세 번 이어 실패했다: 이 뒤로 시세 흐름의 모든 걸음이 비정상이다.
    for _ in 0..3 {
        trading.note_data(false);
    }
    let watching = [Symbol::new("005930")];
    trading.after_feed(true, &watching);
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::Operating,
        "비정상 한 바퀴로는 멈추지 않는다"
    );
    trading.after_feed(true, &watching);
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::AnomalyHalt,
        "비정상 두 바퀴째에 멈춘다"
    );

    // 멈춘 채로 비정상이 이어진다(이 바퀴들은 세지 않는다).
    for _ in 0..5 {
        trading.after_feed(true, &watching);
    }
    // 이상이 풀려 쉬는 중이 되고 다시 운용 중이 된다(사람이 다시 시작한 뒤와 같다).
    store
        .change_team_state(TeamChange {
            to: TeamState::Resting,
            at: millis(at),
            cause: TeamCause::AnomalyCleared,
            detail: "시험: 이상이 풀림".to_string(),
            owner_approved: false,
        })
        .expect("이상 감지 멈춤에서 쉬는 중으로");
    assert_eq!(
        trading.decide_day(DayPlan::Trade).unwrap(),
        TeamState::Operating
    );

    trading.after_feed(true, &watching);
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::Operating,
        "풀린 뒤에는 바퀴를 새로 세어, 비정상 한 바퀴로는 다시 멈추지 않는다"
    );
    trading.after_feed(true, &watching);
    assert_eq!(
        store.team_state().unwrap(),
        TeamState::AnomalyHalt,
        "두 바퀴째에 다시 멈춘다"
    );
    tokio::time::timeout(Duration::from_secs(10), app.shutdown())
        .await
        .expect("시간 안에 꺼져야 한다")
        .expect("끄기가 성공해야 한다");
}
