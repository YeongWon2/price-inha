//! pi-store: docs/dev/modules/store.md 를 따른다.
//!
//! SQLite 파일 하나에 투자 팀 상태 · 매매 · 이벤트 · 접속 기록을 둔다.
//!
//! - 동기 API다. 비동기 코드에서는 호출하는 쪽이 `spawn_blocking`으로 감싼다.
//! - 시각은 모두 호출하는 쪽이 넘긴다. 저장소 안에서 지금 시각을 읽지 않는다.
//! - 기록(팀 상태 기록 · 이벤트 · 접속 기록)은 쌓기만 한다. 지우거나 고치는 API를 만들지 않는다.
//! - 비밀(키 · 토큰 · 비밀번호)을 받는 칸이나 API를 만들지 않는다.
//! - 거부된 호출은 아무것도 바꾸지 않는다.

mod codec;
mod error;
mod log;
mod model;
mod schema;
mod team;
mod trade;

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusqlite::{Connection, TransactionBehavior};

pub use error::{DenyReason, StoreError, Transition};
pub use model::{
    AccessRecord, AccountKind, Amount, Approval, BlockKind, BlockReason, Event, EventRange,
    Judgment, NewAccess, NewTrade, Rationale, RationaleField, RoleScore, SourceRef, StoreInfo,
    TeamCause, TeamChange, TeamState, TeamStateRecord, TimedEvent, Trade, TradeId, TradeState,
    UnixMillis,
};

/// 이 프로그램이 아는 가장 새 스키마 버전. 1부터 시작하고 스키마를 바꿀 때마다 1씩 올린다.
pub const SCHEMA_VERSION: u32 = 2;

/// 이벤트 종류 이름의 가장 긴 길이(글자 수). (3차)
///
/// 이름은 `^[a-z][a-z0-9_.-]*$`이고 이 길이 이하여야 한다. 원본(Hermes 팀 논의 방 로그)의 규칙과 같다.
pub const EVENT_KIND_MAX_CHARS: usize = 64;

/// 저장소 손잡이. 복제하면 같은 파일을 함께 쓴다. 스레드 사이에 넘겨도 된다.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl Store {
    // ───────────── 열기와 이전 ─────────────

    /// SQLite 파일을 연다. 없으면 만든다.
    ///
    /// - 스키마 버전을 읽어 [`SCHEMA_VERSION`]까지 순서대로 올린다. 이미 최신이면 아무것도 바꾸지 않는다.
    /// - WAL(`journal_mode=wal`)과 외래 키(`foreign_keys=on`)를 켠다.
    /// - 다시 열면 자료가 그대로다. 앞선 손잡이가 닫히지 않고 사라졌어도(갑자기 꺼짐) 그대로다.
    /// - SQLite 파일이 아니면 패닉하지 않고 오류를 준다.
    /// - (3차) Unix에서는 DB 파일과 딸린 파일(`-wal` · `-shm`)의 권한을 0600으로 만든다.
    ///   이미 있는 파일의 권한이 더 넓으면 좁힌다. Unix가 아닌 곳에서는 건너뛴다.
    /// - (3차) 버전 2 이하의 파일을 버전 3으로 올릴 때, 이미 있는 이벤트 가운데 내용이
    ///   `{"at": 숫자, "payload": …}` 모양인 줄은 그 `at`이 이벤트의 시각이 된다. 내용은 그대로다.
    pub fn open(path: impl AsRef<Path>) -> Result<Store, StoreError> {
        let conn = schema::open(path.as_ref())?;
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// 열린 저장소의 스키마 버전과 설정.
    pub fn info(&self) -> Result<StoreInfo, StoreError> {
        self.read(schema::info)
    }

    // ───────────── 투자 팀 (M1) ─────────────

    /// 지금 투자 팀 상태. 새 저장소에서는 [`TeamState::Resting`]이다.
    pub fn team_state(&self) -> Result<TeamState, StoreError> {
        self.read(team::current)
    }

    /// 투자 팀 상태를 바꾸고 기록 한 줄을 남긴다.
    ///
    /// 허용 표는 [`TeamState`]에 있다. 거부되면 [`StoreError::TransitionDenied`]이고
    /// 상태와 기록은 그대로다.
    ///
    /// - 표에 없는 전이: [`DenyReason::NotAllowed`]
    /// - 전체 정지에서 나가는데 `owner_approved`가 거짓: [`DenyReason::OwnerApprovalRequired`]
    /// - 하루 한도 멈춤에서 쉬는 중으로 나가는데 사유가 `NextTradingDayStart`가 아님: [`DenyReason::NextTradingDayOnly`]
    /// - 운용 중 · 하루 한도 멈춤 · 이상 감지 멈춤에서 전체 정지로 가는데 사유가 `MaxLoss`가 아님:
    ///   [`DenyReason::MaxLossOnly`] (2차)
    pub fn change_team_state(&self, change: TeamChange) -> Result<TeamStateRecord, StoreError> {
        self.write(|conn| team::change(conn, change))
    }

    /// 투자 팀 상태 변경 기록 전체. 번호 순서다. 새 저장소에서는 비어 있다.
    pub fn team_history(&self) -> Result<Vec<TeamStateRecord>, StoreError> {
        self.read(team::history)
    }

    // ───────────── 매매 (M2) ─────────────

    /// 매매를 논의중으로 만든다. 근거 · 점수 · 승인은 비어 있다.
    pub fn create_trade(&self, new: NewTrade) -> Result<Trade, StoreError> {
        self.write(|conn| trade::create(conn, new))
    }

    /// 매매 한 건. 없으면 [`StoreError::TradeNotFound`].
    pub fn get_trade(&self, id: TradeId) -> Result<Trade, StoreError> {
        self.read(|conn| trade::get(conn, id))
    }

    /// 그 상태에 있는 매매들. 번호 순서다.
    pub fn trades_in_state(&self, state: TradeState) -> Result<Vec<Trade>, StoreError> {
        self.read(|conn| trade::in_state(conn, state))
    }

    /// 논의중 → 판단됨. 근거 · 자료 시각 · 담당 점수 · 합의 점수를 함께 저장한다.
    ///
    /// - 자료 하나라도 `known_at`이 `decided_at`보다 늦으면 [`StoreError::SourceAfterDecision`]이고
    ///   아무것도 저장하지 않는다(매매는 논의중으로 남는다). 같은 시각은 된다.
    /// - 논의중이 아니면 [`StoreError::TransitionDenied`]([`DenyReason::NotAllowed`]).
    /// - 근거에 빈 칸이 있어도 판단됨은 된다.
    /// - `updated_at`은 `decided_at`이 된다.
    pub fn judge_trade(&self, id: TradeId, judgment: Judgment) -> Result<Trade, StoreError> {
        self.write(|conn| trade::judge(conn, id, judgment))
    }

    /// 매매 상태를 바꾼다. 허용 표는 [`TradeState`]에 있다.
    ///
    /// - 표에 없는 전이: [`StoreError::TransitionDenied`]([`DenyReason::NotAllowed`]).
    ///   `to`가 판단됨이면 언제나 거부다(판단됨은 [`Store::judge_trade`]로만 간다).
    /// - `to`가 막힘이면 언제나 거부다(막힘은 [`Store::block_trade`]로만 간다). 판단됨 · 승인대기 ·
    ///   주문됨에서는 [`DenyReason::BlockReasonRequired`]다. (2차)
    /// - 주문됨으로 가는데 근거가 하나라도 비었으면
    ///   [`StoreError::TransitionDenied`]([`DenyReason::RationaleIncomplete`]).
    /// - 거부되면 매매는 그대로다.
    pub fn transition_trade(
        &self,
        id: TradeId,
        to: TradeState,
        at: UnixMillis,
    ) -> Result<Trade, StoreError> {
        self.write(|conn| trade::transition(conn, id, to, at))
    }

    /// 매매를 막힘으로 보내고 막힌 이유를 남긴다. 막힘으로 가는 길은 이것 하나다. (2차)
    ///
    /// 이유 종류와 출발 상태의 짝이 맞아야 한다:
    /// 판단됨 ↔ [`BlockKind::PrincipleOrRationale`], 승인대기 ↔ [`BlockKind::ApprovalRejected`],
    /// 주문됨 ↔ [`BlockKind::UnfilledCancelled`].
    ///
    /// - 짝이 맞지 않으면 [`StoreError::TransitionDenied`]([`DenyReason::BlockReasonMismatch`]).
    /// - 판단됨 · 승인대기 · 주문됨이 아닌 상태에서는
    ///   [`StoreError::TransitionDenied`]([`DenyReason::NotAllowed`]).
    /// - 거부되면 매매는 그대로다. 되면 `updated_at`은 `at`이고, 읽은 매매의 `blocked`에 이유가 나온다.
    /// - "하나도 체결되지 않았는가"는 저장소가 알 수 없다. [`BlockKind::UnfilledCancelled`]를 주기 전에
    ///   체결이 하나도 없음을 확인하는 것은 호출하는 쪽 책임이다.
    pub fn block_trade(
        &self,
        id: TradeId,
        reason: BlockReason,
        at: UnixMillis,
    ) -> Result<Trade, StoreError> {
        self.write(|conn| trade::block(conn, id, reason, at))
    }

    /// 승인 번호와 내용 지문을 매매에 붙인다. 이미 있으면 바꿔 넣는다.
    ///
    /// 2차:
    /// - 주문됨 이후(주문됨 · 보유중 · 팔림 · 복기됨)의 매매에는 붙이거나 바꿀 수 없다:
    ///   [`StoreError::ApprovalLocked`].
    ///   미체결 취소로 막힌 매매도 주문됨을 지나왔으므로 같다(이때 `state`는 막힘이다).
    /// - 한 번 매매에 붙은 번호는 그 매매의 것으로 남는다. 주문되거나 막히거나 다른 번호로 바꿔 넣은
    ///   뒤에도 다른 매매는 그 번호를 쓸 수 없다.
    /// - 다른 매매에 이미 붙은 승인 번호는 붙일 수 없다: [`StoreError::ApprovalNumberReused`].
    /// - 거부되면 두 매매 모두 그대로다.
    pub fn set_trade_approval(
        &self,
        id: TradeId,
        approval: Approval,
        at: UnixMillis,
    ) -> Result<Trade, StoreError> {
        self.write(|conn| trade::set_approval(conn, id, approval, at))
    }

    /// 보유중인 매매의 손절값을 좁힌다(지금 값보다 높게).
    ///
    /// - 지금 값보다 높지 않으면(같거나 낮으면) [`StoreError::StopNotTightened`].
    /// - 보유중이 아니면 [`StoreError::StopNotAdjustable`].
    /// - 거부되면 손절값은 그대로다.
    pub fn update_stop(
        &self,
        id: TradeId,
        new_stop: Amount,
        at: UnixMillis,
    ) -> Result<Trade, StoreError> {
        self.write(|conn| trade::update_stop(conn, id, new_stop, at))
    }

    // ───────────── 이벤트 (C2 · E3) ─────────────

    /// 이벤트를 쌓고 번호를 준다. 번호는 1부터 빠짐없이 커진다.
    /// 여러 스레드가 함께 불러도 번호가 겹치거나 비지 않는다. 다시 열어도 이어진다.
    ///
    /// 3차:
    /// - 시각 없는 이벤트로 쌓는다(내용이 `{"at": …, "payload": …}` 모양이어도 시각 칸은 비어 있다).
    ///   시각을 함께 남기려면 [`Store::append_at`]을 쓴다.
    /// - 종류 이름이 규칙(`^[a-z][a-z0-9_.-]*$`, [`EVENT_KIND_MAX_CHARS`]자 이하)에 어긋나면
    ///   [`StoreError::InvalidEventKind`]이고 아무것도 쌓지 않는다(번호를 쓰지 않는다).
    pub fn append(&self, kind: &str, payload: &serde_json::Value) -> Result<u64, StoreError> {
        self.write(|conn| log::append(conn, kind, payload))
    }

    /// 이벤트를 시각과 함께 쌓고 번호를 준다. (3차)
    ///
    /// - 번호는 [`Store::append`]와 함께 센다(1부터 빠짐없이).
    /// - 내용은 넣은 그대로 저장한다(시각을 내용에 싸 넣지 않는다). 시각은 칸에 따로 둔다.
    /// - 종류 이름 규칙은 [`Store::append`]와 같다. 어긋나면 [`StoreError::InvalidEventKind`]이고
    ///   번호를 쓰지 않는다.
    /// - 시각은 호출하는 쪽이 넘긴다. 앞선 이벤트보다 이른 시각이어도 받는다(번호가 순서다).
    pub fn append_at(
        &self,
        kind: &str,
        payload: &serde_json::Value,
        at: UnixMillis,
    ) -> Result<u64, StoreError> {
        let _ = (kind, payload, at);
        todo!("3차: 시각과 함께 쌓기")
    }

    /// 번호가 `after`보다 큰 이벤트를 번호 순서로 `max`개까지 준다.
    /// `after`가 0이면 처음부터다. 없으면 빈 목록이다.
    pub fn read_after(&self, after: u64, max: usize) -> Result<Vec<Event>, StoreError> {
        self.read(|conn| log::read_after(conn, after, max))
    }

    /// 범위에 드는 이벤트를 시각과 함께, 번호 순서로 `max`개까지 준다. (3차)
    ///
    /// - 범위는 [`EventRange`]다. 기본값(`EventRange::default()`)은 모든 이벤트다.
    /// - 시각 조건(`after_at` · `before_at`)을 주면 시각 없는 이벤트는 나오지 않는다.
    /// - 순서는 언제나 번호 순서다(시각 순서가 아니다).
    /// - 규칙이 생기기 전에 저장된 옛 이름의 이벤트도 그대로 나온다.
    pub fn read_events(
        &self,
        range: EventRange,
        max: usize,
    ) -> Result<Vec<TimedEvent>, StoreError> {
        let _ = (range, max);
        todo!("3차: 번호와 시각 범위로 읽기")
    }

    // ───────────── 접속 기록 (M6) ─────────────

    /// 접속 기록 한 줄을 쌓고 번호를 준다(1부터 빠짐없이).
    pub fn record_access(&self, access: NewAccess) -> Result<u64, StoreError> {
        self.write(|conn| log::record_access(conn, access))
    }

    /// 접속 기록 전체. 번호 순서다.
    pub fn access_log(&self) -> Result<Vec<AccessRecord>, StoreError> {
        self.read(log::access_log)
    }

    // ───────────── 안에서만 쓰는 것 ─────────────

    /// 연결을 잡는다. 잡고 있던 스레드가 패닉했어도 쓴다:
    /// 끝나지 않은 묶음은 그때 되돌려졌으므로 연결에는 반쯤 쓴 것이 없다.
    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// 읽기만 하는 일을 한 묶음(같은 시점의 자료)으로 한다.
    fn read<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let conn = self.lock();
        let tx = conn.unchecked_transaction()?;
        work(&tx)
    }

    /// 쓰는 일을 한 묶음으로 한다. 오류가 나면 묶음 전체를 되돌려 아무것도 바뀌지 않는다.
    fn write<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let mut conn = self.lock();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let out = work(&tx)?;
        tx.commit()?;
        Ok(out)
    }
}
