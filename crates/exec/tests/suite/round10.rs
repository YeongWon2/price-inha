//! 10차: 답 없음 줄임 길은 승인 표시를 받지 않는다(E22 · 결정 14), 증권사 보유를 매매에 맞출 때
//! 시장도 같아야 한다(E16), 같은 매매에 `submit_reduced` 둘을 동시에 불러도 주문은 하나뿐이다(E22 · C10),
//! 장부 없는 매매가 한 종목에 둘 이상일 때의 지금 동작을 잠근다(V6).

use std::sync::Barrier;

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use pi_exec::{Error, ReducedSubmission, SellPurpose, Submission};
use pi_market::{
    Currency, Market, MarketCalendar, Quote, Session, SessionKind, Symbol, TradingDay,
};
use pi_paper::{
    AccountKind, ApprovedOrder, Broker, OrderGroup, OrderRequest, OrderType, PaperConfig, Side,
    UnfilledTimeout,
};
use pi_store::{StoreError, TradeId, TradeState, UnixMillis};

use crate::common::{Bench, SAM, approved, at, config, d, guard, monday};
use crate::reduced::desk::{approvals, awaiting, reduced, todo_of, unanswered, untouched, view};

/// 알림이 갔다.
const SENT: Option<pi_approval::Notice> = Some(pi_approval::Notice::Sent);

// ── 1. 승인 표시는 줄임 길로 못 낸다 ───────────────────────────────────────

/// 승인 표시로 만든 줄임 주문(요청 전량).
fn granted_reduced(
    bench: &Bench,
    id: TradeId,
    number: &pi_approval::ApprovalNumber,
    quantity: &str,
) -> ReducedSubmission {
    ReducedSubmission {
        permit: bench
            .authority
            .approve_granted_for_trade(id.0, approved(Side::Buy, quantity, "70000")),
        ..reduced(bench, id, number, quantity, "70000")
    }
}

// 검증: E22 E33 C10
#[tokio::test]
async fn granted_permit_is_refused_by_submit_reduced() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let submission = granted_reduced(&bench, id, &number, "10");
    assert!(submission.permit.granted());

    let result = bench.exec.submit_reduced(submission).await;

    assert!(matches!(result, Err(Error::GrantedPermit)), "{result:?}");
    untouched(&bench, id).await;
}

// 검증: E22 E33
#[tokio::test]
async fn granted_permit_is_refused_even_when_the_quantity_is_reduced() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);

    let result = bench
        .exec
        .submit_reduced(granted_reduced(&bench, id, &number, "4"))
        .await;

    assert!(matches!(result, Err(Error::GrantedPermit)), "{result:?}");
    untouched(&bench, id).await;
}

// 검증: E22
#[tokio::test]
async fn refusing_a_granted_permit_leaves_the_approval_record_and_the_path_open() {
    let bench = Bench::new().await;
    let id = awaiting(&bench);
    let number = unanswered(&bench, id, 10, "70000", SENT);
    let before = view(&bench, &number);

    let refused = bench
        .exec
        .submit_reduced(granted_reduced(&bench, id, &number, "10"))
        .await;

    assert!(matches!(refused, Err(Error::GrantedPermit)), "{refused:?}");
    let after = view(&bench, &number);
    assert_eq!(after.status, before.status);
    assert_eq!(after.consumed, before.consumed);
    assert_eq!(approvals(&bench).due_handling(&number).unwrap(), None);
    assert!(todo_of(&bench, &number).is_some());
    // 거부는 아무것도 바꾸지 않아 줄임 길의 표시로는 그대로 낼 수 있다.
    bench
        .exec
        .submit_reduced(reduced(&bench, id, &number, "4", "70000"))
        .await
        .unwrap();
    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// 검증: E22 E33 C10
#[tokio::test]
async fn granted_permit_with_a_consumed_approval_still_goes_through_submit() {
    let bench = Bench::new().await;
    let id = bench.judged();
    bench
        .store
        .transition_trade(id, TradeState::AwaitingApproval, UnixMillis(1))
        .unwrap();
    let submission = Submission {
        permit: bench
            .authority
            .approve_granted_for_trade(id.0, approved(Side::Buy, "10", "70000")),
        approval: Some(bench.grant(id, 10, "70000", true)),
        ..bench.buy(id, "10", "70000")
    };

    bench.exec.submit(submission).await.unwrap();

    assert_eq!(bench.trade(id).state, TradeState::Ordered);
    assert_eq!(bench.orders(OrderGroup::Open).await.len(), 1);
}

// ── 2. 증권사 보유를 매매에 맞출 때 시장도 본다 ────────────────────────────

fn utc(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.from_utc_datetime(&date.and_hms_opt(hour, minute, 0).unwrap())
}

/// 그 날짜가 기준일인 미국 달력(정규장 14:30~21:00 UTC).
fn us_calendar(date: NaiveDate) -> MarketCalendar {
    let day = |date: NaiveDate| TradingDay {
        date,
        sessions: vec![Session {
            kind: SessionKind::Regular,
            start: utc(date, 14, 30),
            end: utc(date, 21, 0),
        }],
    };
    MarketCalendar {
        market: Market::Us,
        today: day(date),
        previous_business_day: day(date - Duration::days(3)),
        next_business_day: day(date + Duration::days(1)),
    }
}

/// 미국 시장의 같은 글자 종목(`005930`)을 `quantity`주 사서 체결시킨다(시계는 미국 장 안으로 간다).
async fn own_us_namesake(bench: &Bench, quantity: &str) {
    let now = utc(monday(), 15, 0);
    bench.set_at(now);
    bench.exec.broker().set_calendar(us_calendar(monday()));
    let order = ApprovedOrder {
        market: Market::Us,
        symbol: Symbol::new(SAM),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d(quantity),
        price: Some(d("70000.00")),
        account_kind: AccountKind::Paper,
    };
    let request = OrderRequest {
        client_order_id: None,
        market: Market::Us,
        symbol: Symbol::new(SAM),
        side: Side::Buy,
        order_type: OrderType::Limit,
        quantity: d(quantity),
        price: Some(d("70000.00")),
        account_kind: AccountKind::Paper,
        timeout: UnfilledTimeout::LONG_TERM,
        confirm_high_value: false,
        permit: Some(bench.authority.approve(order)),
    };
    let broker: &dyn Broker = bench.exec.broker();
    broker.place_order(request, now).await.unwrap();
    bench.set_at(now + Duration::seconds(1));
    bench
        .exec
        .on_quote(&Quote {
            symbol: Symbol::new(SAM),
            price: d("70000.00"),
            currency: Currency::Usd,
            exchange_time: Some(bench.now()),
            received_at: bench.now(),
        })
        .unwrap();
    let held = bench.holdings().await;
    assert!(
        held.iter().any(|h| h.market == Market::Us
            && h.symbol.as_str() == SAM
            && h.quantity == d(quantity)),
        "{held:?}"
    );
}

/// 끝난 주문을 모의계좌에서 덜어 내고 저장한다(장부를 보유로 다시 만들게 하려는 준비).
fn archive_all(bench: &Bench) {
    let later = bench.now() + Duration::seconds(1);
    let archived = bench.exec.broker().archive_before(later);
    assert!(!archived.orders.is_empty());
    bench.exec.on_time().unwrap();
}

/// 저장된 스냅샷의 장부에서 `keep`인 매매만 남긴다(나머지는 장부 없는 매매가 된다).
fn keep_ledger_of(bench: &Bench, keep: &[TradeId]) {
    let mut doc = bench.store.latest_paper_snapshot().unwrap().unwrap().state;
    let ledger = doc["pi_exec"]["positions"].as_array_mut().unwrap();
    ledger.retain(|item| keep.iter().any(|id| item["trade_id"] == id.0));
    bench
        .store
        .save_paper_snapshot(&doc, UnixMillis(1))
        .unwrap();
}

/// 같은 글자 미국 종목을 살 달러가 넉넉한 설정(기준가가 종목 글자로 걸려 미국 주문도 70,000 언저리여야 한다).
fn rich() -> PaperConfig {
    let mut config = config();
    config.initial_cash.usd = d("10000000");
    config
}

/// 다시 켜고(국내 시계로 돌아와) 증권사와 맞춰 본다.
async fn restart(bench: &mut Bench) {
    bench.reopen(rich());
    bench.set_at(at(10));
    bench.exec.reconcile().await.unwrap();
}

/// 파는 수량이 `left`를 넘으면 막히는가.
async fn sell_over(bench: &Bench, id: TradeId, quantity: &str, left: &str) {
    let result = bench
        .exec
        .sell(bench.sell(id, SellPurpose::TakeProfit, quantity, "70000"))
        .await;
    assert!(
        matches!(result, Err(Error::SellTooLarge { remaining }) if remaining == d(left)),
        "{result:?}"
    );
}

// 검증: E16 V6
#[tokio::test]
async fn holding_of_another_market_with_the_same_symbol_is_not_adopted() {
    let mut bench = Bench::with(rich()).await;
    // 보유중으로 저장돼 있지만 국내 보유는 없고, 같은 글자의 미국 종목만 계좌에 있다.
    let id = bench.judged();
    bench
        .store
        .transition_trade(id, TradeState::Ordered, UnixMillis(1))
        .unwrap();
    bench
        .store
        .transition_trade(id, TradeState::Holding, UnixMillis(2))
        .unwrap();
    own_us_namesake(&bench, "3").await;
    archive_all(&bench);

    restart(&mut bench).await;

    assert_eq!(bench.trade(id).state, TradeState::Holding);
    assert!(bench.exec.guards().is_empty(), "{:?}", bench.exec.guards());
}

// 검증: E16 V6
#[tokio::test]
async fn domestic_holding_is_adopted_with_its_own_quantity_beside_a_namesake() {
    let mut bench = Bench::with(rich()).await;
    let id = bench.holding().await;
    own_us_namesake(&bench, "3").await;
    archive_all(&bench);
    keep_ledger_of(&bench, &[]);

    restart(&mut bench).await;

    assert_eq!(bench.exec.guards(), vec![guard(id, "10")]);
    sell_over(&bench, id, "11", "10").await;
}

// ── 3. 같은 매매에 submit_reduced 둘을 동시에 ──────────────────────────────

// 검증: E22 C10
#[test]
fn two_reduced_submissions_at_once_order_only_one() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let (bench, id, number) = runtime.block_on(async {
        let bench = Bench::new().await;
        let id = awaiting(&bench);
        let number = unanswered(&bench, id, 10, "70000", SENT);
        (bench, id, number)
    });
    let start = Barrier::new(2);

    let results: Vec<Result<_, Error>> = std::thread::scope(|scope| {
        let racers: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .build()
                        .unwrap();
                    let submission = reduced(&bench, id, &number, "4", "70000");
                    start.wait();
                    runtime.block_on(bench.exec.submit_reduced(submission))
                })
            })
            .collect();
        racers.into_iter().map(|r| r.join().unwrap()).collect()
    });

    // 주문은 하나뿐이다. 진 쪽은 저장소가 전이를 막은 오류(`Store(TransitionDenied)`)이거나, 이긴 쪽이
    // 이미 끝낸 뒤에 와서 본 `NotAwaitingApproval(Ordered)`다(어느 쪽이든 주문은 나가지 않는다).
    let won = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(won, 1, "{results:?}");
    let lost = results.iter().find_map(|r| r.as_ref().err()).unwrap();
    assert!(
        matches!(
            lost,
            Error::Store(StoreError::TransitionDenied { .. })
                | Error::NotAwaitingApproval(TradeState::Ordered)
        ),
        "{lost:?}"
    );
    runtime.block_on(async {
        assert_eq!(bench.trade(id).state, TradeState::Ordered);
        let open = bench.orders(OrderGroup::Open).await;
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].quantity, d("4"));
        assert!(bench.orders(OrderGroup::Closed).await.is_empty());
    });
}

// ── 4. 한 종목에 장부 없는 매매가 둘 이상(지금 동작을 잠근다) ──────────────

// 검증: V6
#[tokio::test]
async fn orphans_of_one_symbol_each_take_the_whole_holding_as_their_cap() {
    let mut bench = Bench::new().await;
    let first = bench.holding().await;
    let second = bench.holding().await;
    archive_all(&bench);
    keep_ledger_of(&bench, &[]);

    restart(&mut bench).await;

    // 각자 계좌의 그 종목 보유 전부(20주)를 상한으로 본다. 계좌보다 많이 파는 것은 모의계좌가 막는다.
    assert_eq!(
        bench.exec.guards(),
        vec![guard(first, "20"), guard(second, "20")]
    );
    sell_over(&bench, first, "21", "20").await;
    sell_over(&bench, second, "21", "20").await;
}

// 검증: V6
#[tokio::test]
async fn orphan_beside_a_ledgered_trade_takes_only_what_the_ledger_leaves() {
    let mut bench = Bench::new().await;
    let kept = bench.holding().await;
    let orphan = bench.holding().await;
    archive_all(&bench);
    keep_ledger_of(&bench, &[kept]);

    restart(&mut bench).await;

    assert_eq!(
        bench.exec.guards(),
        vec![guard(kept, "10"), guard(orphan, "10")]
    );
    sell_over(&bench, kept, "11", "10").await;
    sell_over(&bench, orphan, "11", "10").await;
}
