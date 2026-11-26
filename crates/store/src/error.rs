//! 저장소 오류. 오류 타입은 [`StoreError`] 하나다.

use crate::approval::{ApprovalId, ApprovalStatus};
use crate::fact::FactId;
use crate::memo::DecisionMemoId;
use crate::model::{Amount, RationaleField, TeamState, TradeId, TradeState, UnixMillis};
use crate::notice::NoticeId;
use crate::proposal::{ProposalId, ProposalState};
use crate::receipt::TriggerReceiptKey;
use crate::room::RoomActorKind;
use crate::session::SessionId;
use crate::template::{TemplateId, TemplateState};

/// 거부된 전이가 무엇이었는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// 투자 팀 상태 전이.
    Team {
        /// 떠나려던 상태.
        from: TeamState,
        /// 가려던 상태.
        to: TeamState,
    },
    /// 매매 상태 전이.
    Trade {
        /// 떠나려던 상태.
        from: TradeState,
        /// 가려던 상태.
        to: TradeState,
    },
    /// 템플릿 상태 전이. (9차)
    Template {
        /// 떠나려던 상태.
        from: TemplateState,
        /// 가려던 상태.
        to: TemplateState,
    },
    /// 규칙 개선안 상태 전이. (9차)
    Proposal {
        /// 떠나려던 상태.
        from: ProposalState,
        /// 가려던 상태.
        to: ProposalState,
    },
    /// 승인 요청 결과 전이(기다림에서 한 번만). (9차)
    Approval {
        /// 지금 결과.
        from: ApprovalStatus,
        /// 바꾸려던 결과.
        to: ApprovalStatus,
    },
}

/// 전이가 거부된 이유. 종류가 늘어날 수 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DenyReason {
    /// 허용 표에 없는 전이다(같은 상태로 가는 것도 여기에 든다).
    NotAllowed,
    /// 전체 정지에서 나가려는데 "내 승인" 표시가 없다.
    OwnerApprovalRequired,
    /// 하루 한도 멈춤은 "다음 거래일 시작" 사유로만 풀린다.
    NextTradingDayOnly,
    /// 근거가 비어 주문됨으로 갈 수 없다. `missing`에는 빈 칸이 모두 들어간다.
    RationaleIncomplete {
        /// 빈 칸들. [`RationaleField`]를 적은 순서(가설 · 계기 · 들어간 값 · 손절값 · 목표값 · 기대값)다.
        missing: Vec<RationaleField>,
    },
    /// 막힘은 이유를 받는 [`crate::Store::block_trade`]로만 간다.
    /// 이유를 안 받는 [`crate::Store::transition_trade`]로 막힘을 주면 이것으로 거부된다.
    BlockReasonRequired,
    /// 막힌 이유 종류와 출발 상태의 짝이 맞지 않는다
    /// (판단됨 ↔ 원칙 · 근거 부족, 승인대기 ↔ 승인 거절, 주문됨 ↔ 미체결 취소).
    BlockReasonMismatch,
    /// 전체 정지로 가는 사유는 최대 손실([`crate::TeamCause::MaxLoss`])이나 내 정지([`crate::TeamCause::OwnerStop`])뿐이다.
    MaxLossOnly,
    /// 투자 팀이 전체 정지인 동안에는 실전 승격을 하지 않는다. (후보 일괄 처리)
    TeamFullStop,
}

/// 저장소의 모든 오류.
///
/// 상태 전이 거부는 [`StoreError::TransitionDenied`]로만 나온다.
/// 거부된 호출은 아무것도 바꾸지 않는다(상태 · 기록 · 근거 모두 그대로).
/// 종류가 늘어날 수 있다.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// 상태 전이가 거부됐다.
    #[error("허용되지 않은 상태 전이: {transition:?} ({reason:?})")]
    TransitionDenied {
        /// 거부된 전이.
        transition: Transition,
        /// 거부된 이유.
        reason: DenyReason,
    },
    /// 판단에 쓴 자료의 "알게 된 시각"이 판단 시각보다 늦다. 판단은 저장되지 않는다.
    #[error(
        "자료 '{source_name}'의 알게 된 시각({known_at:?})이 판단 시각({decided_at:?})보다 늦다"
    )]
    SourceAfterDecision {
        /// 늦은 자료의 이름(여럿이면 준 순서에서 처음 것).
        source_name: String,
        /// 그 자료의 알게 된 시각.
        known_at: UnixMillis,
        /// 판단 시각.
        decided_at: UnixMillis,
    },
    /// 손절값을 좁히는 쪽(더 높게)이 아닌 값으로 바꾸려 했다.
    #[error("손절값은 좁히는 쪽으로만 바꿀 수 있다: 지금 {current:?}, 요청 {requested:?}")]
    StopNotTightened {
        /// 지금 손절값.
        current: Amount,
        /// 바꾸려던 값.
        requested: Amount,
    },
    /// 보유중이 아닌 매매의 손절값을 바꾸려 했다.
    #[error("보유중이 아닌 매매의 손절값은 바꿀 수 없다: {state:?}")]
    StopNotAdjustable {
        /// 그 매매의 지금 상태.
        state: TradeState,
    },
    /// 주문됨 이후(주문됨 · 보유중 · 팔림 · 복기됨)의 매매에 승인을 붙이거나 바꾸려 했다.
    #[error("주문된 뒤에는 승인을 붙이거나 바꿀 수 없다: {state:?}")]
    ApprovalLocked {
        /// 그 매매의 지금 상태.
        state: TradeState,
    },
    /// 다른 매매에 이미 붙은 승인 번호를 붙이려 했다(승인 번호는 다시 쓰지 않는다).
    #[error("승인 번호 {number}는 이미 다른 매매에 붙어 있다")]
    ApprovalNumberReused {
        /// 다시 쓰려던 승인 번호.
        number: i64,
    },
    /// 이벤트 종류 이름이 규칙(`^[a-z][a-z0-9_.-]*$`, [`crate::EVENT_KIND_MAX_CHARS`]자 이하)에
    /// 어긋난다. 이벤트는 쌓이지 않고 번호도 쓰지 않는다. (3차)
    #[error("이벤트 종류 이름이 규칙에 어긋난다: {kind:?}")]
    InvalidEventKind {
        /// 거부된 이름. 받은 이름(앞뒤 공백을 떼기 전) 그대로다.
        ///
        /// (4차) 받은 이름이 [`crate::REJECTED_KIND_KEEP_CHARS`]자보다 길면 앞
        /// [`crate::REJECTED_KIND_KEEP_CHARS`]자 뒤에 `…`(U+2026) 한 글자를 붙여 담는다(글자 단위로
        /// 자르므로 여러 바이트 글자가 깨지지 않는다). 그 이하이면 그대로 담는다.
        kind: String,
    },
    /// 그 번호의 템플릿이 없다. (9차)
    #[error("템플릿을 찾을 수 없다: {0:?}")]
    TemplateNotFound(TemplateId),
    /// 모의중이 아닌 템플릿의 값을 바꾸려 했다. (9차)
    #[error("모의중이 아닌 템플릿의 값은 바꿀 수 없다: {state:?}")]
    TemplateLocked {
        /// 그 템플릿의 지금 상태.
        state: TemplateState,
    },
    /// 그 번호의 규칙 개선안이 없다. (9차)
    #[error("규칙 개선안을 찾을 수 없다: {0:?}")]
    ProposalNotFound(ProposalId),
    /// 규칙 개선안의 근거 매매가 모자란다(서로 다른 매매 [`crate::PROPOSAL_MIN_EVIDENCE`]건 이상). (9차)
    #[error("규칙 개선안의 근거 매매가 {count}건뿐이다")]
    ProposalEvidenceTooFew {
        /// 받은 서로 다른 근거 매매의 수.
        count: usize,
    },
    /// 그 번호의 승인 요청이 없다. (9차)
    #[error("승인 요청을 찾을 수 없다: {0:?}")]
    ApprovalNotFound(ApprovalId),
    /// 승인이 아닌(기다림 · 거절 · 답 없음 · 만료) 요청을 실행에 쓰려 했다. (9차)
    #[error("승인 {id:?}는 쓸 수 없다: {status:?}")]
    ApprovalNotUsable {
        /// 승인 번호.
        id: ApprovalId,
        /// 그 요청의 지금 결과.
        status: ApprovalStatus,
    },
    /// 이미 실행에 쓴 승인을 또 쓰려 했다(한 번만 소비). (9차)
    #[error("승인 {id:?}는 이미 썼다")]
    ApprovalAlreadyConsumed {
        /// 승인 번호.
        id: ApprovalId,
    },
    /// 효력이 끝난(`expires_at` 이후) 승인을 쓰려 했다. (9차)
    #[error("승인 {id:?}의 효력이 끝났다")]
    ApprovalExpired {
        /// 승인 번호.
        id: ApprovalId,
    },
    /// 실행할 내용의 지문이 승인한 내용의 지문과 다르다. (9차)
    #[error("승인 {id:?}의 내용 지문이 실행할 내용과 다르다")]
    ApprovalFingerprintMismatch {
        /// 승인 번호.
        id: ApprovalId,
    },
    /// 접속 키 해시의 소금이 짧다([`crate::ACCESS_KEY_SALT_MIN_BYTES`]바이트 이상). (9차)
    #[error("접속 키 소금이 {len}바이트뿐이다")]
    AccessKeySaltTooShort {
        /// 받은 소금의 길이(바이트).
        len: usize,
    },
    /// 그 번호의 세션이 없다. (10차)
    #[error("세션을 찾을 수 없다: {0:?}")]
    SessionNotFound(SessionId),
    /// 끝낸 세션에 쓰거나 다시 끝내려 했다. (10차)
    #[error("세션 {0:?}는 이미 끝났다")]
    SessionEnded(SessionId),
    /// 메시지 규칙([`crate::NewMessage`])에 어긋난다. 묶음 전체를 쓰지 않았다. (10차)
    #[error("메시지 묶음의 {index}번째가 규칙에 어긋난다")]
    InvalidMessage {
        /// 처음 어긴 메시지의 자리(0부터).
        index: usize,
    },
    /// 쓴 쪽의 종류가 그 방 이벤트 종류를 쓸 수 없다. (10차)
    #[error("{actor_kind:?}는 방 이벤트 '{kind}'를 쓸 수 없다")]
    RoomEventNotAllowed {
        /// 쓴 쪽의 종류.
        actor_kind: RoomActorKind,
        /// 쓰려던 이벤트 종류.
        kind: String,
    },
    /// 같은 방에 같은 `event_id`로 다른 내용이 이미 있다. (10차)
    #[error("방 '{room}'의 이벤트 '{event_id}'가 다른 내용으로 이미 있다")]
    RoomEventConflict {
        /// 방.
        room: String,
        /// 겹친 멱등 키.
        event_id: String,
    },
    /// 그 키의 트리거 영수증이 없다. (10차)
    #[error("트리거 영수증이 없다: {0:?}")]
    TriggerReceiptNotFound(TriggerReceiptKey),
    /// 트리거 영수증을 실행 중으로 닫으려 했다. (10차)
    #[error("트리거 영수증은 실행 중으로 닫을 수 없다: {0:?}")]
    TriggerReceiptRunningClose(TriggerReceiptKey),
    /// 그 번호의 자료가 없다. (10차)
    #[error("자료를 찾을 수 없다: {0:?}")]
    FactNotFound(FactId),
    /// 판단 시각이 없는(판단 전) 매매에 근거 자료를 붙이려 했다. (10차)
    #[error("매매 {id:?}는 아직 판단되지 않았다: {state:?}")]
    TradeNotJudged {
        /// 매매 번호.
        id: TradeId,
        /// 그 매매의 지금 상태.
        state: TradeState,
    },
    /// 매매의 판단 시각보다 늦게 알게 된 자료를 근거로 붙이려 했다(V36). 아무것도 붙이지 않았다. (10차)
    #[error("자료 {fact:?}의 알게 된 시각({known_at:?})이 판단 시각({decided_at:?})보다 늦다")]
    FactAfterDecision {
        /// 늦은 자료(준 순서에서 처음 것).
        fact: FactId,
        /// 그 자료의 알게 된 시각.
        known_at: UnixMillis,
        /// 매매의 판단 시각.
        decided_at: UnixMillis,
    },
    /// 지식 기록의 값이 규칙에 어긋난다(빈 기록 키 · 빈 조각 · 빈 메타 키). 아무것도 바꾸지 않았다. (11차)
    #[error("지식 값이 규칙에 어긋난다: {0}")]
    InvalidKnowledge(String),
    /// 임베딩 벡터가 비었거나 유한하지 않은 값이 있다. (11차)
    #[error("임베딩 벡터가 규칙에 어긋난다")]
    InvalidEmbedding,
    /// 그 번호의 조각이 없다. (11차)
    #[error("조각을 찾을 수 없다: {0:?}")]
    ChunkNotFound(crate::ChunkId),
    /// 웹 푸시 구독의 값이 규칙에 어긋난다(https가 아닌 주소 · 빈 키). (11차)
    #[error("웹 푸시 구독이 규칙에 어긋난다")]
    InvalidPushSubscription,
    /// 스냅샷을 뜰 자리에 이미 파일이 있다. 그 파일은 건드리지 않았다. (11차)
    #[error("스냅샷 자리에 이미 파일이 있다: {}", .0.display())]
    SnapshotTargetExists(std::path::PathBuf),
    /// 매매 수량이 0이거나 너무 크다. (후보 일괄 처리)
    #[error("매매 수량은 1 이상이어야 한다")]
    InvalidQuantity,
    /// 승인이 붙었거나 승인대기를 지난 매매의 수량을 두거나 바꾸려 했다. (후보 일괄 처리)
    #[error("이 매매의 수량은 더 바꿀 수 없다: {state:?}")]
    QuantityLocked {
        /// 그 매매의 지금 상태.
        state: TradeState,
    },
    /// 제안됨이 아닌 개선안의 위험 표시를 바꾸려 했다. (후보 일괄 처리)
    #[error("제안됨이 아닌 개선안의 위험 표시는 바꿀 수 없다: {id:?} {state:?}")]
    ProposalLocked {
        /// 개선안 번호.
        id: ProposalId,
        /// 그 개선안의 지금 상태.
        state: ProposalState,
    },
    /// 실전 승격을 확인한 판과 템플릿의 지금 판이 다르다. 템플릿도 승인도 그대로다. (후보 일괄 처리)
    #[error("템플릿 {id:?}의 판이 다르다: 확인한 판 {expected}, 지금 판 {actual}")]
    TemplateRevisionMismatch {
        /// 템플릿 번호.
        id: TemplateId,
        /// 호출하는 쪽이 확인한 판.
        expected: u32,
        /// 지금 판.
        actual: u32,
    },
    /// 통과 기록의 판과 템플릿의 지금 판이 달라 실전으로 옮길 수 없다. 템플릿도 승인도 그대로다. (16차)
    #[error(
        "템플릿 {id:?}의 통과 기록 판 {pass_revision:?}이 지금 판 {revision}과 달라 실전으로 옮길 수 없다"
    )]
    TemplatePassStale {
        /// 템플릿 번호.
        id: TemplateId,
        /// 통과 기록의 판(기록이 없으면 `None`).
        pass_revision: Option<u32>,
        /// 지금 판.
        revision: u32,
    },
    /// 내 캐릭터 값이 규칙에 맞지 않는다(이름 0자 · 51자 이상, 빈 외형). 아무것도 바꾸지 않았다. (19차)
    #[error("내 캐릭터 값이 맞지 않다: {0}")]
    InvalidPlayer(String),
    /// 담당 의견 · 팀장 요약이 규칙에 맞지 않는다. 판단도 의견도 남기지 않았다. (19차)
    #[error("담당 의견이 맞지 않다: {0}")]
    InvalidOpinion(String),
    /// 공지 · 공지 확인 값이 규칙에 맞지 않는다(글 0자 · 501자 이상, 기다림으로 답하기). (19차)
    #[error("공지 값이 맞지 않다: {0}")]
    InvalidNotice(String),
    /// 그 번호의 공지가 없다. (19차)
    #[error("공지를 찾을 수 없다: {0:?}")]
    NoticeNotFound(NoticeId),
    /// 그 공지에 그 담당의 확인 줄이 없다. (19차)
    #[error("공지 {notice:?}에 담당 {role}의 확인이 없다")]
    NoticeAckNotFound {
        /// 공지 번호.
        notice: NoticeId,
        /// 담당 이름.
        role: String,
    },
    /// 그 담당의 공지 확인은 이미 기다림에서 바뀌었다(한 번만 바뀐다). (19차)
    #[error("공지 {notice:?}에 담당 {role}의 확인은 이미 바뀌었다")]
    NoticeAckLocked {
        /// 공지 번호.
        notice: NoticeId,
        /// 담당 이름.
        role: String,
    },
    /// 그 매매의 결정 메모가 이미 있다(매매마다 하나). (19차)
    #[error("매매 {0:?}의 결정 메모가 이미 있다")]
    DecisionMemoExists(TradeId),
    /// 그 번호의 결정 메모가 없다. (19차)
    #[error("결정 메모를 찾을 수 없다: {0:?}")]
    DecisionMemoNotFound(DecisionMemoId),
    /// 결정 메모가 이미 정산 대기에서 바뀌었다(한 번만 바뀐다). (19차)
    #[error("결정 메모 {0:?}는 이미 정산 대기가 아니다")]
    DecisionMemoLocked(DecisionMemoId),
    /// 결정 메모 값이 규칙에 맞지 않는다(빈 교훈 등). (19차)
    #[error("결정 메모 값이 맞지 않다: {0}")]
    InvalidDecisionMemo(String),
    /// (21차, 결정 31) 보관으로 옮기려던 줄이 내보낸 것과 다르다(줄 수 · 지문), 또는 보관 묶음 값이 규칙에
    /// 어긋난다. 아무것도 지우지 않았다.
    #[error("보관 묶음이 맞지 않다: {0}")]
    ArchiveMismatch(String),
    /// 그 번호의 매매가 없다.
    #[error("매매를 찾을 수 없다: {0:?}")]
    TradeNotFound(TradeId),
    /// 열려는 파일을 이 저장소로 쓸 수 없다: 이 저장소가 만든 파일이 아니거나(다른 표가 있거나
    /// 표시가 다르다), 이 프로그램이 아는 것보다 새 스키마이거나, 스키마 버전 값이 이상하거나,
    /// WAL을 켤 수 없다.
    ///
    /// (4차) 저장 파일을 만들거나 권한을 맞추는 파일 시스템 오류는 여기에 담지 않는다:
    /// [`StoreError::FileAccess`].
    #[error("저장소 파일을 쓸 수 없다: {0}")]
    Incompatible(String),
    /// 저장 파일을 만들거나 그 권한을 보거나 좁히는 파일 시스템 작업이 실패했다
    /// (예: 쓸 수 없는 폴더에 새 저장소를 열려 했다). (4차)
    #[error("저장 파일을 다룰 수 없다: {}: {source}", path.display())]
    FileAccess {
        /// 다루려던 파일(본 파일이나 딸린 파일 `-wal` · `-shm`).
        path: std::path::PathBuf,
        /// 파일 시스템 오류.
        #[source]
        source: std::io::Error,
    },
    /// SQLite 오류.
    #[error("SQLite 오류: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// JSON 오류.
    #[error("JSON 오류: {0}")]
    Json(#[from] serde_json::Error),
}
