//! 저장소가 주고받는 값. 로직은 없다.

/// 시각. 유닉스 기준 밀리초. 호출하는 쪽이 넘긴다(저장소는 시계를 읽지 않는다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixMillis(pub i64);

/// 금액 · 가격. 통화의 정수 최소 단위다(부동소수점을 쓰지 않는다).
/// `i64` 전체 범위가 한 자리도 틀리지 않고 저장되고 읽혀야 한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(pub i64);

/// 매매 번호. 저장소가 붙인다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TradeId(pub i64);

// ───────────── 투자 팀 (M1) ─────────────

/// 투자 팀 상태(`spec.md` 상태표). 새 저장소의 처음 상태는 [`TeamState::Resting`]이다.
///
/// 허용 전이(이 밖은 모두 거부, 같은 상태로 가는 것도 거부):
///
/// | 에서 | 으로 | 조건 |
/// |---|---|---|
/// | 쉬는 중 | 운용 중 | |
/// | 쉬는 중 | 이상 감지 멈춤 | |
/// | 운용 중 | 쉬는 중 | |
/// | 운용 중 | 하루 한도 멈춤 | |
/// | 운용 중 | 이상 감지 멈춤 | |
/// | 운용 중 | 전체 정지 | 사유가 [`TeamCause::MaxLoss`]일 때만 |
/// | 하루 한도 멈춤 | 쉬는 중 | 사유가 [`TeamCause::NextTradingDayStart`]일 때만 |
/// | 하루 한도 멈춤 | 전체 정지 | 사유가 [`TeamCause::MaxLoss`]일 때만 |
/// | 이상 감지 멈춤 | 쉬는 중 | |
/// | 이상 감지 멈춤 | 전체 정지 | 사유가 [`TeamCause::MaxLoss`]일 때만 |
/// | 전체 정지 | 쉬는 중 | `owner_approved`가 참일 때만 |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamState {
    /// 운용 중
    Operating,
    /// 쉬는 중
    Resting,
    /// 하루 한도 멈춤
    DailyLimitHalt,
    /// 이상 감지 멈춤
    AnomalyHalt,
    /// 전체 정지
    FullStop,
}

impl TeamState {
    /// 모든 상태.
    pub const ALL: [TeamState; 5] = [
        TeamState::Operating,
        TeamState::Resting,
        TeamState::DailyLimitHalt,
        TeamState::AnomalyHalt,
        TeamState::FullStop,
    ];
}

/// 투자 팀 상태가 바뀐 사유.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamCause {
    /// 오늘 매매하기로 해 운용을 시작한다.
    TradingStart,
    /// 장이 끝났다.
    MarketClose,
    /// 다음 거래일이 시작됐다. 하루 한도 멈춤은 이 사유로만 풀린다.
    NextTradingDayStart,
    /// 하루 손실 한도에 닿았다.
    DailyLossLimit,
    /// 최대 손실에 닿았다.
    MaxLoss,
    /// 시세 지연 · 오류 · 계좌 불일치를 감지했다.
    AnomalyDetected,
    /// 이상이 풀렸다.
    AnomalyCleared,
    /// 내가 다시 시작을 눌렀다.
    OwnerRestart,
}

impl TeamCause {
    /// 모든 사유.
    pub const ALL: [TeamCause; 8] = [
        TeamCause::TradingStart,
        TeamCause::MarketClose,
        TeamCause::NextTradingDayStart,
        TeamCause::DailyLossLimit,
        TeamCause::MaxLoss,
        TeamCause::AnomalyDetected,
        TeamCause::AnomalyCleared,
        TeamCause::OwnerRestart,
    ];
}

/// 투자 팀 상태를 바꾸라는 요청.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamChange {
    /// 가려는 상태.
    pub to: TeamState,
    /// 언제.
    pub at: UnixMillis,
    /// 왜(기계가 보는 사유).
    pub cause: TeamCause,
    /// 왜(사람이 읽는 설명). 그대로 기록에 남는다.
    pub detail: String,
    /// "내 승인" 표시. 전체 정지에서 나갈 때 참이어야 한다.
    pub owner_approved: bool,
}

/// 투자 팀 상태 변경 기록 한 줄. 지우거나 고치는 API는 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamStateRecord {
    /// 1부터 빠짐없이 커지는 번호.
    pub seq: u64,
    /// 언제.
    pub at: UnixMillis,
    /// 무엇에서.
    pub from: TeamState,
    /// 무엇으로.
    pub to: TeamState,
    /// 왜(기계가 보는 사유).
    pub cause: TeamCause,
    /// 왜(사람이 읽는 설명). 요청에 준 글자 그대로다.
    pub detail: String,
    /// 요청에 있던 "내 승인" 표시.
    pub owner_approved: bool,
}

// ───────────── 매매 (M2) ─────────────

/// 매매 상태(`spec.md` 상태표). 새 매매는 [`TradeState::Discussing`]이다.
///
/// 허용 전이(이 밖은 모두 거부, 같은 상태로 가는 것도 거부):
///
/// | 에서 | 으로 | 조건 |
/// |---|---|---|
/// | 논의중 | 판단됨 | `Store::judge_trade`로만 |
/// | 판단됨 | 승인대기 | |
/// | 판단됨 | 막힘 | `Store::block_trade`로만, 이유는 [`BlockKind::PrincipleOrRationale`] |
/// | 판단됨 | 주문됨 | 근거 6칸이 모두 차 있을 때만 |
/// | 승인대기 | 주문됨 | 근거 6칸이 모두 차 있을 때만 |
/// | 승인대기 | 막힘 | `Store::block_trade`로만, 이유는 [`BlockKind::ApprovalRejected`] |
/// | 주문됨 | 보유중 | |
/// | 주문됨 | 막힘 | `Store::block_trade`로만, 이유는 [`BlockKind::UnfilledCancelled`] |
/// | 보유중 | 팔림 | |
/// | 팔림 | 복기됨 | |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TradeState {
    /// 논의중
    Discussing,
    /// 판단됨
    Judged,
    /// 승인대기
    AwaitingApproval,
    /// 막힘
    Blocked,
    /// 주문됨
    Ordered,
    /// 보유중
    Holding,
    /// 팔림
    Sold,
    /// 복기됨
    Reviewed,
}

impl TradeState {
    /// 모든 상태.
    pub const ALL: [TradeState; 8] = [
        TradeState::Discussing,
        TradeState::Judged,
        TradeState::AwaitingApproval,
        TradeState::Blocked,
        TradeState::Ordered,
        TradeState::Holding,
        TradeState::Sold,
        TradeState::Reviewed,
    ];
}

/// 매매가 막힌 이유의 종류. 종류마다 출발 상태가 하나로 정해져 있다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// 원칙 · 근거 부족. 판단됨에서만.
    PrincipleOrRationale,
    /// 내가 승인을 거절했다. 승인대기에서만.
    ApprovalRejected,
    /// 정한 시간 안에 하나도 체결되지 않아 주문을 취소했다. 주문됨에서만.
    UnfilledCancelled,
}

impl BlockKind {
    /// 모든 종류.
    pub const ALL: [BlockKind; 3] = [
        BlockKind::PrincipleOrRationale,
        BlockKind::ApprovalRejected,
        BlockKind::UnfilledCancelled,
    ];
}

/// 매매가 막힌 이유. 막힌 매매를 읽으면 함께 나온다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockReason {
    /// 이유 종류.
    pub kind: BlockKind,
    /// 사람이 읽는 설명. 준 글자 그대로 남는다.
    pub detail: String,
}

/// 계좌 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccountKind {
    /// 모의
    Paper,
    /// 실전
    Live,
}

/// 근거의 칸 이름.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RationaleField {
    /// 가설
    Hypothesis,
    /// 계기
    Trigger,
    /// 들어간 값
    Entry,
    /// 손절값
    Stop,
    /// 목표값
    Target,
    /// 기대값
    ExpectedValue,
}

/// 매매의 근거. 칸이 `None`이거나 글자 칸이 비었거나 공백뿐이면 "빈 것"이다.
/// 하나라도 비면 그 매매는 주문됨으로 갈 수 없다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rationale {
    /// 가설
    pub hypothesis: Option<String>,
    /// 계기
    pub trigger: Option<String>,
    /// 들어간 값
    pub entry: Option<Amount>,
    /// 손절값
    pub stop: Option<Amount>,
    /// 목표값
    pub target: Option<Amount>,
    /// 기대값
    pub expected_value: Option<Amount>,
}

/// 판단에 쓴 자료 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    /// 자료 이름(무엇인지 알아볼 수 있는 글자).
    pub name: String,
    /// 알게 된 시각. 판단 시각보다 늦을 수 없다(같은 것은 된다).
    pub known_at: UnixMillis,
}

/// 담당 한 명의 점수.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleScore {
    /// 담당 이름.
    pub role: String,
    /// 점수.
    pub score: i32,
    /// 점수의 근거.
    pub reason: String,
}

/// 팀장의 판단. 논의중 → 판단됨으로 갈 때 함께 저장된다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Judgment {
    /// 판단 시각.
    pub decided_at: UnixMillis,
    /// 근거(빈 칸이 있어도 저장은 된다. 주문됨으로만 못 간다).
    pub rationale: Rationale,
    /// 판단에 쓴 자료들. 준 순서대로 저장된다.
    pub sources: Vec<SourceRef>,
    /// 담당 점수들. 준 순서대로 저장된다.
    pub scores: Vec<RoleScore>,
    /// 합의 점수.
    pub consensus_score: i32,
}

/// 승인 번호와 승인한 내용의 지문.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    /// 승인 번호.
    pub number: i64,
    /// 승인한 내용의 지문(해시 글자).
    pub content_fingerprint: String,
}

/// 새 매매.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTrade {
    /// 종목 코드.
    pub symbol: String,
    /// 계좌 종류.
    pub account: AccountKind,
    /// 만든 시각.
    pub at: UnixMillis,
}

/// 저장된 매매 한 건.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trade {
    /// 매매 번호.
    pub id: TradeId,
    /// 종목 코드.
    pub symbol: String,
    /// 계좌 종류. 만든 뒤에는 바뀌지 않는다.
    pub account: AccountKind,
    /// 지금 상태.
    pub state: TradeState,
    /// 만든 시각.
    pub created_at: UnixMillis,
    /// 마지막으로 바뀐 시각(전이 · 손절값 변경 · 승인 붙이기의 `at`). 처음에는 `created_at`과 같다.
    pub updated_at: UnixMillis,
    /// 근거. 판단 전에는 모두 `None`이다.
    pub rationale: Rationale,
    /// 판단 시각. 판단 전에는 `None`.
    pub decided_at: Option<UnixMillis>,
    /// 판단에 쓴 자료들.
    pub sources: Vec<SourceRef>,
    /// 담당 점수들.
    pub scores: Vec<RoleScore>,
    /// 합의 점수. 판단 전에는 `None`.
    pub consensus_score: Option<i32>,
    /// 승인. 붙이기 전에는 `None`.
    pub approval: Option<Approval>,
    /// 막힌 이유. [`crate::Store::block_trade`]로 막힌 매매에만 있다.
    pub blocked: Option<BlockReason>,
}

// ───────────── 이벤트 (C2 · E3) ─────────────

/// 이벤트 한 건.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// 1부터 빠짐없이 커지는 번호. 다시 열어도 이어진다.
    pub seq: u64,
    /// 종류.
    pub kind: String,
    /// 내용. 넣은 JSON과 같다.
    pub payload: serde_json::Value,
}

/// 시각이 함께 나오는 이벤트 한 건. [`crate::Store::read_events`]가 준다. (3차)
///
/// [`Event`]는 다른 모듈이 값으로 만들기 때문에 칸을 더하지 않고 이 타입을 따로 둔다.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedEvent {
    /// 번호. [`Event::seq`]와 같다.
    pub seq: u64,
    /// 종류.
    pub kind: String,
    /// 이벤트의 시각. [`crate::Store::append_at`]으로 쌓았으면 그 시각이다.
    /// [`crate::Store::append`]로 쌓았거나 버전 3 이전의 줄이면 없다
    /// (이전할 때 내용이 `{"at": 숫자, "payload": …}` 모양이던 줄은 그 `at`이다).
    pub at: Option<UnixMillis>,
    /// 내용. 넣은 JSON과 같다.
    pub payload: serde_json::Value,
}

/// 이벤트를 읽을 범위. 준 조건을 모두 만족하는 이벤트만 나온다. (3차)
///
/// 기본값은 조건이 없는 것(모든 이벤트)이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EventRange {
    /// 번호가 이보다 큰 것만. 0이면 처음부터다.
    pub after_seq: u64,
    /// 시각이 이보다 늦은 것만(같은 시각은 빠진다). 시각 없는 이벤트는 빠진다.
    pub after_at: Option<UnixMillis>,
    /// 시각이 이보다 이른 것만(같은 시각은 빠진다). 시각 없는 이벤트는 빠진다.
    pub before_at: Option<UnixMillis>,
}

// ───────────── 접속 기록 (M6) ─────────────

/// 새 접속 기록.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAccess {
    /// 언제.
    pub at: UnixMillis,
    /// 어디서(주소나 기기 이름).
    pub origin: String,
    /// 무엇을 했는지.
    pub action: String,
}

/// 저장된 접속 기록 한 줄. 지우거나 고치는 API는 없다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRecord {
    /// 1부터 빠짐없이 커지는 번호.
    pub seq: u64,
    /// 언제.
    pub at: UnixMillis,
    /// 어디서.
    pub origin: String,
    /// 무엇을 했는지.
    pub action: String,
}

// ───────────── 저장소 정보 ─────────────

/// 열린 저장소의 설정.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreInfo {
    /// 파일에 적용된 스키마 버전. 열고 나면 [`crate::SCHEMA_VERSION`]과 같다.
    pub schema_version: u32,
    /// `PRAGMA journal_mode`의 값(소문자). `"wal"`이어야 한다.
    pub journal_mode: String,
    /// `PRAGMA foreign_keys`가 켜져 있는가.
    pub foreign_keys: bool,
}
