//! 답 없는 사기 시험의 도우미: 승인대기 매매 · 승인 요청 · 알림 기록 · 기한 넘기기.
//!
//! 승인 기록은 `pi-approval`의 공개 API로만 만든다(조립하는 쪽이 하는 일을 흉내 낸다).

use std::sync::Arc;

use pi_approval::{
    ApprovalNumber, ApprovalView, Approvals, Ask, DueAction, Notice, OrderSubject,
    OrderType as AskOrderType, Requested, Side as AskSide, SignalKind, Subject, WaitTimes,
};
use pi_exec::ReducedSubmission;
use pi_paper::{AccountKind, OrderGroup, Side, UnfilledTimeout};
use pi_store::{AccountKind as StoreAccount, ApprovalStatus, TradeId, TradeState, UnixMillis};

use crate::common::{Bench, SAM, d};

/// 기다릴 시간이 다 된 때(초). 사는 주문의 기본 기다릴 시간(N2, 5분)이다. 파는 신호(1분 · 5분)도 지난다.
pub const WAIT: i64 = 300;
/// 신호 유효 시간의 끝(초). 요청을 만든 때부터 한 시간이다.
pub const VALID: i64 = 3_600;

/// 승인 모듈에 줄 멈춘 시계.
struct Frozen(UnixMillis);

impl pi_approval::Clock for Frozen {
    fn now(&self) -> UnixMillis {
        self.0
    }
}

/// 책상의 지금 시각(밀리초).
fn millis(bench: &Bench) -> UnixMillis {
    UnixMillis(bench.now().timestamp_millis())
}

/// 책상의 지금 시각에 멈춘 승인 관리자(조립하는 쪽의 관리자 노릇). 부를 때마다 저장소에서 다시 읽는다.
pub fn approvals(bench: &Bench) -> Approvals {
    Approvals::new(
        bench.store.clone(),
        Arc::new(Frozen(millis(bench))),
        WaitTimes::default(),
    )
}

/// 판단됨 매매를 승인대기로 옮긴다.
pub fn wait_for_approval(bench: &Bench, id: TradeId) {
    bench
        .store
        .transition_trade(id, TradeState::AwaitingApproval, millis(bench))
        .unwrap();
}

/// 승인대기인 삼성전자 매매(모의 계좌).
pub fn awaiting(bench: &Bench) -> TradeId {
    let id = bench.judged();
    wait_for_approval(bench, id);
    id
}

/// 그 매매의 삼성전자 지정가 사기 요청 내용. 신호 id는 `trade-<매매 번호>`(조립하는 쪽의 약속),
/// 신호 유효 시간의 끝은 지금 + [`VALID`]초다.
pub fn buying(bench: &Bench, trade_id: TradeId, quantity: u64, price: &str) -> OrderSubject {
    OrderSubject {
        account: StoreAccount::Paper,
        symbol: SAM.to_string(),
        side: AskSide::Buy,
        quantity,
        price: d(price),
        order_type: AskOrderType::Limit,
        signal_id: format!("trade-{}", trade_id.0),
        signal_kind: SignalKind::Buy,
        signal_valid_until: UnixMillis(millis(bench).0 + VALID * 1_000),
    }
}

/// 승인 요청을 만들고, 알림 결과가 있으면 남긴다.
pub fn ask(bench: &Bench, subject: OrderSubject, notice: Option<Notice>) -> Requested {
    let approvals = approvals(bench);
    let requested = approvals
        .request(Ask {
            subject: Subject::Order(subject),
            reason: "금액 한도를 넘는 사기".to_string(),
        })
        .unwrap();
    if let Some(notice) = notice {
        approvals.record_notice(&requested.number, notice).unwrap();
    }
    requested
}

/// 시계를 기준 시각에서 `seconds`초 뒤로 옮기고 기한이 된 기다림 요청을 닫는다.
pub fn close_at(bench: &Bench, seconds: i64) {
    bench.set(seconds);
    approvals(bench).expire_due(millis(bench)).unwrap();
}

/// 그 내용의 요청을 만들고(알림 결과를 남기고) 기다릴 시간이 다 돼 답 없음으로 닫힌 것의 번호.
pub fn unanswered_with(
    bench: &Bench,
    subject: OrderSubject,
    notice: Option<Notice>,
) -> ApprovalNumber {
    let number = ask(bench, subject, notice).number;
    close_at(bench, WAIT);
    assert_eq!(view(bench, &number).status, ApprovalStatus::NoAnswer);
    number
}

/// 그 매매의 삼성전자 지정가 사기 요청이 답 없음으로 닫힌 것의 번호.
pub fn unanswered(
    bench: &Bench,
    trade_id: TradeId,
    quantity: u64,
    price: &str,
    notice: Option<Notice>,
) -> ApprovalNumber {
    unanswered_with(bench, buying(bench, trade_id, quantity, price), notice)
}

/// 승인 기록 한 건(있어야 한다).
pub fn view(bench: &Bench, number: &ApprovalNumber) -> ApprovalView {
    approvals(bench).get(number).unwrap().unwrap()
}

/// 처리 기록이 없는 닫힌 요청 목록에서 그 번호의 할 일. 목록에 없으면 `None`.
pub fn todo_of(bench: &Bench, number: &ApprovalNumber) -> Option<DueAction> {
    approvals(bench)
        .unhandled_due()
        .unwrap()
        .into_iter()
        .find(|due| &due.number == number)
        .map(|due| due.action)
}

/// 줄인 사는 주문 하나(모의 템플릿, 그 매매에 묶인 삼성전자 지정가 표시, 60초 미체결 취소).
pub fn reduced(
    bench: &Bench,
    trade_id: TradeId,
    number: &ApprovalNumber,
    quantity: &str,
    price: &str,
) -> ReducedSubmission {
    ReducedSubmission {
        trade_id,
        permit: bench.permit(trade_id, Side::Buy, quantity, price),
        template_account: AccountKind::Paper,
        unanswered: number.clone(),
        timeout: UnfilledTimeout::SCALP,
    }
}

/// 증권사에 주문이 하나도 없다(열린 것도 끝난 것도).
pub async fn nothing_sent(bench: &Bench) {
    assert!(bench.orders(OrderGroup::Open).await.is_empty());
    assert!(bench.orders(OrderGroup::Closed).await.is_empty());
}

/// 거부된 뒤: 매매는 승인대기 그대로이고 주문이 없다.
pub async fn untouched(bench: &Bench, id: TradeId) {
    assert_eq!(bench.trade(id).state, TradeState::AwaitingApproval);
    nothing_sent(bench).await;
}
