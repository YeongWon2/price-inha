//! 승인 관리(A7): 요청(C12) · 답(C7) · 소비 · 답 없음(E22) · 다시 켠 뒤.
//!
//! 원본: OpenClaw `operator_approvals`의 상태 기계(`docs/research/triggers-approval-memory-deep.md` 3.2),
//! 같은 요청 합치기(Hermes · DeskRPG, 3.4). 저장은 `pi-store`의 승인 표를 쓴다.
//!
//! 의도해서 다름: 답 없음의 처리(FN2 3b), 다시 켠 뒤 신호 유효 시간 안의 기다림 유지(결정 6번),
//! 승인 대상에 실전 승격 · 규칙 개선 · 다시 시작을 더함.

use std::sync::Arc;
use std::time::Duration;

use pi_store::{ApprovalStatus, Store, UnixMillis};

use crate::error::{Error, RejectReason};
use crate::subject::{Fingerprint, Subject};

/// 시계 포트. 이 크레이트는 시스템 시계를 직접 읽지 않는다(ARCH-CLOCK-2).
pub trait Clock: Send + Sync {
    /// 지금 시각.
    fn now(&self) -> UnixMillis;
}

/// 승인 번호. 요청마다 무작위로 만들고 다시 쓰지 않는다(M5).
///
/// 만든 번호는 16바이트 무작위 값의 소문자 16진수 32자다. 저장소의 줄 번호와는 다르다.
/// 화면이 보낸 글자는 [`ApprovalNumber::new`]로 감싼다(모양이 틀리면 그냥 "없는 번호"다).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApprovalNumber(String);

impl ApprovalNumber {
    /// 받은 글자를 그대로 감싼다. 검사하지 않는다.
    pub fn new(text: impl Into<String>) -> ApprovalNumber {
        let _ = text;
        todo!()
    }

    /// 글자.
    pub fn as_str(&self) -> &str {
        todo!()
    }
}

/// 신호 종류별 기다릴 시간(N2). 템플릿에서 바꾼다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaitTimes {
    /// 손절 신호. 기본 1분.
    pub stop_loss: Duration,
    /// 익절 신호. 기본 5분.
    pub take_profit: Duration,
    /// 사는 주문. 기본 5분.
    pub buy: Duration,
}

impl Default for WaitTimes {
    /// N2 기본값: 손절 1분, 익절 5분, 사는 주문 5분.
    fn default() -> WaitTimes {
        todo!()
    }
}

/// 승인 요청 만들기의 입력(C12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// 무엇을 승인하는지.
    pub subject: Subject,
    /// 왜 묻는지(사람이 읽는 쉬운 말).
    pub reason: String,
}

/// 만든(또는 합친) 승인 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requested {
    /// 승인 번호.
    pub number: ApprovalNumber,
    /// 내용 지문.
    pub fingerprint: Fingerprint,
    /// 지문 앞 6자. 카드에 보인다.
    pub short_code: String,
    /// 이 시각까지 답이 없으면 답 없음(주문만). 만든 시각 + 신호 종류별 기다릴 시간(N2)이다.
    /// 실전 승격 · 규칙 개선 · 다시 시작은 없음(답이 올 때까지 기다린다, V42).
    pub wait_until: Option<UnixMillis>,
    /// 이 시각부터 만료(주문만: 신호 유효 시간의 끝). 그 밖의 대상은 없음.
    pub expires_at: Option<UnixMillis>,
    /// 같은 대상 · 같은 지문으로 요청된 횟수. 새로 만들면 1, 기다리는 중인 요청에 합치면 1씩 오른다.
    pub repeat: u32,
}

/// 사용자의 답.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decision {
    /// 승인.
    Approve,
    /// 거절.
    Reject,
}

impl Decision {
    /// 화면이 보낸 답 글자를 읽는다. 정확히 `"approve"`일 때만 승인이고,
    /// `"reject"`를 포함해 그 밖의 모든 글자(모르는 답)는 거절이다(P19).
    pub fn from_wire(text: &str) -> Decision {
        let _ = text;
        todo!()
    }
}

/// 답하기(C7)의 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// 승인됐다(진행됨).
    Approved,
    /// 거절로 남았다(거절됨). 팀이 대신 판단하지 않는다.
    Rejected(RejectReason),
}

/// 승인 요청 알림을 보낸 결과. 알림은 다른 모듈(notify)이 보내고 여기에 알린다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Notice {
    /// 알림이 갔다.
    Sent,
    /// 알림 보내기에 실패했다(승인 경로가 막힘, P20).
    Failed,
}

/// 답이 없을 때(또는 만료될 때) 호출하는 쪽이 할 일(E22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DueAction {
    /// 사는 주문이고 알림이 갔다: 금액 한도 안으로 줄여 다시 점검한다(FN2 3b3).
    ReduceWithinLimit,
    /// 사는 주문인데 알림이 갔다는 기록이 없다(실패했거나 알리지 않음): 사지 않는다(P20, 결정 5번).
    DoNotBuy,
    /// 파는 신호: 팀이 위험 신호인지 익절 신호인지 보고 판단한다(FN2 3b2).
    TeamDecides,
    /// 신호 유효 시간이 지나 만료됐다: 아무것도 하지 않는다.
    Dropped,
}

/// [`Approvals::expire_due`]가 닫은 요청 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueOutcome {
    /// 승인 번호.
    pub number: ApprovalNumber,
    /// 대상.
    pub subject: Subject,
    /// 바뀐 결과: [`ApprovalStatus::NoAnswer`] 또는 [`ApprovalStatus::Expired`].
    pub status: ApprovalStatus,
    /// 할 일.
    pub action: DueAction,
}

/// 승인 요청 한 건의 모습.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalView {
    /// 승인 번호.
    pub number: ApprovalNumber,
    /// 대상.
    pub subject: Subject,
    /// 왜 묻는지.
    pub reason: String,
    /// 지금 결과(저장소의 결과).
    pub status: ApprovalStatus,
    /// 내용 지문.
    pub fingerprint: Fingerprint,
    /// 지문 앞 6자.
    pub short_code: String,
    /// 실행에 썼으면 참.
    pub consumed: bool,
    /// [`Requested::wait_until`]과 같다.
    pub wait_until: Option<UnixMillis>,
    /// [`Requested::expires_at`]과 같다.
    pub expires_at: Option<UnixMillis>,
}

/// 승인 관리자. 스레드 사이에 나눠 써도 된다(`Send + Sync`).
pub struct Approvals {
    store: Store,
    clock: Arc<dyn Clock>,
    waits: WaitTimes,
}

impl Approvals {
    /// 저장소 · 시계 · 기다릴 시간(템플릿 값)으로 만든다.
    pub fn new(store: Store, clock: Arc<dyn Clock>, waits: WaitTimes) -> Approvals {
        let _ = (store, clock, waits);
        todo!()
    }

    /// 승인 요청을 만든다(C12). 시각은 시계에서 읽는다.
    ///
    /// - 주문이 지정가가 아니면 [`Error::LimitOrderRequired`].
    /// - 주문의 방향과 신호 종류의 짝이 안 맞으면 [`Error::InvalidSubject`].
    /// - 주문의 신호 유효 시간이 이미 지났으면(지금 ≥ `signal_valid_until`) [`Error::SignalAlreadyExpired`].
    /// - 전체 정지 중에 실전 승격이면 [`Error::FullStop`]. (다시 시작 · 규칙 개선은 만든다.)
    /// - 같은 대상 · 같은 지문의 요청이 기다리는 중이면 새로 만들지 않고 그 번호를 돌려주며
    ///   `repeat`만 1 올린다.
    /// - 주문의 `wait_until`은 지금 + 신호 종류별 기다릴 시간, `expires_at`은 `signal_valid_until`이다.
    pub fn request(&self, ask: Ask) -> Result<Requested, Error> {
        let _ = ask;
        todo!()
    }

    /// 승인 요청 알림을 보낸 결과를 남긴다. 사는 주문의 답 없음 처리에 쓴다(P20).
    ///
    /// 없는 번호면 [`Error::Denied`]([`RejectReason::NotFound`]).
    pub fn record_notice(&self, number: &ApprovalNumber, notice: Notice) -> Result<(), Error> {
        let _ = (number, notice);
        todo!()
    }

    /// 사용자의 답을 받는다(C7). 시각은 시계에서 읽는다.
    ///
    /// - 거절은 지문을 보지 않고 바로 거절로 남긴다(V43 · P18) → `Rejected(UserRejected)`.
    ///   (기다림이 아니면 아래 E8 규칙을 따른다.)
    /// - 없는 번호 → `Rejected(NotFound)`. 아무것도 바뀌지 않는다(E8).
    /// - 이미 처리된 번호 → `Rejected(AlreadyResolved)`. 첫 결과는 그대로다(E8 · V45).
    /// - 만료(지금 ≥ 만료 시각) → 결과를 만료로 닫고 `Rejected(Expired)`(E8).
    /// - 승인인데 화면 지문(`shown_fingerprint`, 64자 전체)이 요청의 지문과 다르면 결과를 거절로 닫고
    ///   `Rejected(FingerprintMismatch)`(E7 · V44). 앞 6자만 보내도 다르다.
    /// - 그 밖의 승인 → [`Answer::Approved`].
    pub fn answer(
        &self,
        number: &ApprovalNumber,
        shown_fingerprint: &str,
        decision: Decision,
    ) -> Result<Answer, Error> {
        let _ = (number, shown_fingerprint, decision);
        todo!()
    }

    /// 승인을 실행에 쓴다(한 번만). `executing`은 **실행할** 대상이고, 그 지문을 승인한 지문과 비교한다.
    ///
    /// 쓸 수 없으면 [`Error::Denied`]이고 실행하지 않는다:
    /// 없는 번호 `NotFound`, 승인됨이 아님 `NotApproved`, 이미 씀 `AlreadyConsumed`,
    /// 만료 `Expired`, 지문이 다름 `FingerprintMismatch`, 전체 정지 중의 실전 승격 `FullStop`.
    ///
    /// 지문이 달라 거부된 승인은 거절로 바뀐 것으로 다룬다: 그 뒤에는 같은 번호를 어떤 대상으로도 쓸 수 없다(E7).
    /// 동시에 여러 번 불러도 한 번만 통과한다.
    pub fn consume(&self, number: &ApprovalNumber, executing: &Subject) -> Result<(), Error> {
        let _ = (number, executing);
        todo!()
    }

    /// 기한이 된 기다림 요청을 닫고 처리 방침을 돌려준다(E22).
    ///
    /// - 주문이고 `wait_until ≤ now < expires_at`: 결과를 답 없음으로 바꾼다. 사는 주문은 알림이 갔으면
    ///   [`DueAction::ReduceWithinLimit`], 아니면 [`DueAction::DoNotBuy`]. 파는 주문은 [`DueAction::TeamDecides`].
    /// - 주문이고 `now ≥ expires_at`: 만료로 닫고 [`DueAction::Dropped`].
    /// - 실전 승격 · 규칙 개선 · 다시 시작: 바꾸지 않고 돌려주지도 않는다(계속 기다린다, V42).
    /// - 한 번 닫은 요청은 다시 돌려주지 않는다.
    pub fn expire_due(&self, now: UnixMillis) -> Result<Vec<DueOutcome>, Error> {
        let _ = now;
        todo!()
    }

    /// 프로그램을 다시 켰을 때 부른다(결정 6번). 시각은 시계에서 읽는다.
    ///
    /// 기다림 요청 가운데 신호 유효 시간이 지난 것(지금 ≥ 만료 시각)은 만료로 닫고 그 번호들을 돌려준다.
    /// 유효 시간 안인 것과 만료 시각이 없는 것은 기다림으로 둔다.
    pub fn on_restart(&self) -> Result<Vec<ApprovalNumber>, Error> {
        todo!()
    }

    /// 승인 요청 한 건. 없으면 `None`.
    pub fn get(&self, number: &ApprovalNumber) -> Result<Option<ApprovalView>, Error> {
        let _ = number;
        todo!()
    }
}
