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
pub const SCHEMA_VERSION: u32 = 4;

/// 이벤트 종류 이름의 가장 긴 길이(글자 수). (3차)
///
/// 이름은 `^[a-z][a-z0-9_.-]*$`이고 이 길이 이하여야 한다. 원본(Hermes 팀 논의 방 로그)의 규칙과 같다.
pub const EVENT_KIND_MAX_CHARS: usize = 64;

/// 거부한 이벤트 종류 이름을 오류([`StoreError::InvalidEventKind`])에 담을 때 남기는 가장 긴 길이(글자 수). (4차)
///
/// 받은 이름이 이보다 길면 앞 이만큼 뒤에 `…`(U+2026) 한 글자를 붙여 담는다. 이 이하이면 그대로 담는다.
/// 아주 긴 이름이 오류 글과 로그를 부풀리지 않게 한다.
pub const REJECTED_KIND_KEEP_CHARS: usize = 256;

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
    /// - (4차) 스키마 버전 4는 이벤트 시각 칸에 색인을 두고(시각 범위 읽기),
    ///   옛 기록 형식의 경계 번호를 한 번 정해 남긴다([`Store::legacy_envelope_until`]).
    /// - (4차) 저장 파일을 만들거나 권한을 맞추는 파일 시스템 오류는 [`StoreError::FileAccess`]다.
    ///   거부된 열기는 파일을 바꾸지 않는다: 이 저장소의 파일이 아니어서 거부하면 그 파일의 권한도 그대로다.
    ///   심어 둔 심볼릭 링크는 따라가 권한을 바꾸지 않는다.
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
    /// - 원본(Hermes 팀 논의 방 로그)처럼 검사 전에 이름의 앞뒤 공백을 떼어 내고, 떼어 낸 이름으로
    ///   검사하고 저장한다(`" chat "`은 `"chat"`으로 쌓인다). 거부할 때 오류에는 받은 이름 그대로가 담긴다.
    ///   (4차) 다만 받은 이름이 [`REJECTED_KIND_KEEP_CHARS`]자보다 길면 잘라 담는다
    ///   ([`StoreError::InvalidEventKind`]의 `kind`).
    pub fn append(&self, kind: &str, payload: &serde_json::Value) -> Result<u64, StoreError> {
        self.write(|conn| log::append(conn, kind, payload, None))
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
        self.write(|conn| log::append(conn, kind, payload, Some(at)))
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
        self.read(|conn| log::read_events(conn, range, max))
    }

    /// 옛 기록 형식의 경계 번호. (4차, 2026-11-14 사용자 결정 "경계 번호로")
    ///
    /// 이벤트에 시각 칸이 생기기 전(스키마 버전 2 이하)에는 접속 관문이 내용을
    /// `{"at": 시각, "payload": …}` 봉투로 싸서 쌓았다. 이 번호 이하의 줄만 그 옛 봉투일 수 있다.
    /// 그보다 뒤의 줄은 내용 그대로다(봉투 모양이어도 옛 봉투가 아니다). 접속 관문은 이 번호 이하의
    /// 줄만 옛 봉투로 풀고, 그 뒤의 줄은 내용 그대로 보낸다.
    ///
    /// 경계는 파일을 버전 4로 올릴 때 한 번 정해 남긴다:
    ///
    /// - 처음부터 버전 4 이상으로 만든 파일: 없음(`None`).
    /// - 버전 1 · 2 파일을 올릴 때: 그 시점의 마지막 이벤트 번호(시각 칸을 더하기 직전의 것).
    ///   봉투 모양인 줄이 있든 없든 마지막 번호다.
    /// - 이미 버전 3인 파일을 올릴 때(4차 이전에 버전 3으로 올린 파일은 경계를 남기지 않았다):
    ///   **옛 봉투 줄** 가운데 가장 큰 번호. 그런 줄이 없으면 없음(`None`).
    ///   옛 봉투 줄은 접속 관문 4차까지가 실제로 옛 봉투로 풀던 모양과 정확히 같다:
    ///   내용이 **정확히 두 칸**(`at` · `payload`)인 객체이고, 맨 위 칸의 이름이 겹치지 않고,
    ///   `at`이 i64 범위 안의 정수이고 `-0`이 아니고(5차에서 좁힘),
    ///   내용 어디에도 짝 없는 서로게이트 이스케이프(`"\ud800"` 등)가 없고(6차에서 좁힘:
    ///   SQLite는 그 글자를 JSON으로 받지만 접속 관문의 `serde_json`은 거부한다. 관문과 같게 뺀다),
    ///   **시각 칸이 비었거나**(3차 접속 관문이 [`Store::append`]로 봉투 그대로 쌓은 줄)
    ///   **내용의 `at`과 같다**(버전 3으로 올라오며 내용의 `at`을 시각 칸에 옮겨 적은 줄).
    ///   세 칸 이상인 객체, `at`이 정수가 아닌 객체, 내용의 `at`과 다른 시각이 적힌 줄,
    ///   JSON이 아니거나 객체가 아닌 내용은 들지 않는다.
    ///   칸 이름은 접속 관문의 `serde_json`이 읽는 이름, 곧 이스케이프를 푼 이름으로 본다(8차):
    ///   `{"at":1,"payload":2}`는 옛 봉투 줄이고, `{"at":1,"at":2,"payload":3}`은 이름이
    ///   겹친 줄이라 아니다.
    ///   (바로잡음 2026-11-14: 처음 규칙은 "시각 칸이 채워진 봉투 줄"이라 시각 칸이 빈 봉투 줄을 빠뜨렸다.)
    ///   한계: 버전 3에서 [`Store::append_at`]으로 봉투 모양 내용을 내용의 `at`과 같은 시각으로 쌓은
    ///   줄이나 [`Store::append`]로 봉투 모양 내용을 쌓은 줄은 옛 봉투 줄과 파일만 보고는 가를 수
    ///   없어 이 규칙에 함께 든다. 접속 관문 4차도 그 줄을 옛 봉투로 풀었으므로 보내는 모양은 같다.
    ///
    /// 이벤트가 하나도 없는 버전 1 · 2 파일의 경계는 `Some(0)`이다: 옛 형식 파일이었다는 사실은
    /// 남기되 옛 봉투로 풀 줄은 없다. 그 뒤 쌓는 줄(1번부터)은 모두 내용 그대로다.
    ///
    /// 한 번 정한 경계는 바뀌지 않는다: 다시 열거나 이벤트를 더 쌓아도 같고, 기록 표처럼
    /// 저장소 파일에 직접 넣은 SQL(고치기 · 지우기 · 다시 넣기)로도 바뀌지 않는다.
    /// 경계는 표 `legacy_boundary`의 한 줄(`id = 1`, 경계가 없으면 `until_seq`가 빈 칸)에 두고,
    /// 버전 4로 올리는 같은 트랜잭션 안에서 넣은 뒤 그 표의 넣기 · 고치기 · 지우기를 트리거로 막는다.
    /// 이전 도중 실패하면 그 줄도 남지 않고, 고친 뒤 다시 열면 같은 규칙으로 같은 경계를 정한다.
    pub fn legacy_envelope_until(&self) -> Result<Option<u64>, StoreError> {
        self.read(schema::legacy_envelope_until)
    }

    /// 경계 번호 이하의 줄 가운데 **옛 봉투 줄**의 번호. (7차, 접속 관문 7차 6번을 받친다)
    ///
    /// 번호가 경계([`Store::legacy_envelope_until`]) 이하이면서 `after`보다 큰 줄 가운데 옛 봉투 줄인
    /// 것의 번호를 번호 순서로 `max`개까지 준다. `after`가 0이면 처음부터다.
    ///
    /// - 경계가 없으면(`None`) 언제나 빈 목록이다. 경계보다 뒤의 줄은 봉투 모양이어도 나오지 않는다.
    /// - 옛 봉투 줄의 규칙은 [`Store::legacy_envelope_until`] 문서의 것과 한 글자도 다르지 않다:
    ///   내용이 정확히 두 칸(`at` · `payload`)인 객체이고, 맨 위 칸의 이름이 겹치지 않고,
    ///   `at`이 i64 범위 안의 정수이고 `-0`이 아니고, 내용 어디에도 짝 없는 서로게이트 이스케이프가
    ///   없고, 시각 칸이 비었거나 내용의 `at`과 같다.
    /// - 판정은 저장된 원래 글자로 한다. 접속 관문은 이미 읽힌 JSON 값만 받아 맨 위 칸 이름이 겹친
    ///   줄(`{"at":1,"payload":1,"payload":2}` 등)을 옛 봉투와 가를 수 없으므로 이 목록을 쓴다.
    /// - 읽기만 한다. 아무것도 바꾸지 않는다.
    pub fn legacy_envelope_seqs(&self, after: u64, max: usize) -> Result<Vec<u64>, StoreError> {
        self.read(|conn| schema::legacy_envelope_seqs(conn, after, max))
    }

    /// [`Store::legacy_envelope_seqs`]에 훑는 범위의 위 끝을 준 것. (8차, 접속 관문 8차를 받친다)
    ///
    /// 번호가 `after`보다 크고 `upto` 이하이면서 경계([`Store::legacy_envelope_until`]) 이하인 줄
    /// 가운데 옛 봉투 줄의 번호를 번호 순서로 `max`개까지 준다. 경계 뒤의 줄은 `upto`가 커도 늘 빠진다.
    ///
    /// - 경계가 없으면(`None`) 언제나 빈 목록이다. `after >= upto`이거나 `max`가 0이어도 빈 목록이다.
    /// - 옛 봉투 줄의 규칙은 [`Store::legacy_envelope_seqs`]와 한 벌이다(칸 이름은 이스케이프를 푼
    ///   이름으로 보고, 겹침도 푼 이름으로 가른다).
    /// - 결과는 `legacy_envelope_seqs(after, usize::MAX)` 가운데 `upto` 이하인 것의 앞 `max`개와 같다.
    /// - 한 쪽을 읽을 때 `upto`까지만 훑는다: 경계 앞에 옛 봉투가 아닌 줄이 많아도 경계까지 훑지 않는다.
    /// - 읽기만 한다. 아무것도 바꾸지 않는다.
    pub fn legacy_envelope_seqs_upto(
        &self,
        after: u64,
        upto: u64,
        max: usize,
    ) -> Result<Vec<u64>, StoreError> {
        self.read(|conn| schema::legacy_envelope_seqs_upto(conn, after, upto, max))
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
