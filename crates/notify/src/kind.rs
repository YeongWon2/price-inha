//! 알림 종류.

/// 푸시 긴급도(`Urgency` 머리글, RFC 8030).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    /// `very-low`.
    VeryLow,
    /// `low`.
    Low,
    /// `normal`.
    Normal,
    /// `high`.
    High,
}

/// 알림 종류 여섯 가지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoticeKind {
    /// 승인 요청.
    ApprovalRequest,
    /// 멈춤(팀이 멈췄다).
    Stall,
    /// 이상(예: 시세가 끊김).
    Anomaly,
    /// 접속 잠금(E2).
    AccessLock,
    /// 백업 실패.
    BackupFailure,
    /// 하루 정리.
    DailySummary,
}

impl NoticeKind {
    /// 여섯 종류 모두.
    pub const ALL: [NoticeKind; 6] = [
        NoticeKind::ApprovalRequest,
        NoticeKind::Stall,
        NoticeKind::Anomaly,
        NoticeKind::AccessLock,
        NoticeKind::BackupFailure,
        NoticeKind::DailySummary,
    ];

    /// 짧은 이름: `approval_request` `stall` `anomaly` `access_lock` `backup_failure` `daily_summary`.
    pub fn name(self) -> &'static str {
        match self {
            NoticeKind::ApprovalRequest => "approval_request",
            NoticeKind::Stall => "stall",
            NoticeKind::Anomaly => "anomaly",
            NoticeKind::AccessLock => "access_lock",
            NoticeKind::BackupFailure => "backup_failure",
            NoticeKind::DailySummary => "daily_summary",
        }
    }

    /// 저장소 이벤트 종류: `notice.` + [`NoticeKind::name`]. 저장소의 종류 이름 규칙에 맞는다.
    pub fn event_kind(self) -> &'static str {
        match self {
            NoticeKind::ApprovalRequest => "notice.approval_request",
            NoticeKind::Stall => "notice.stall",
            NoticeKind::Anomaly => "notice.anomaly",
            NoticeKind::AccessLock => "notice.access_lock",
            NoticeKind::BackupFailure => "notice.backup_failure",
            NoticeKind::DailySummary => "notice.daily_summary",
        }
    }

    /// 이름으로 종류를 찾는다. 모르는 이름이면 `None`.
    pub fn from_name(name: &str) -> Option<NoticeKind> {
        NoticeKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// 조용한 시간을 무시하는가: 승인 요청과 이상은 무시한다(참). 백업 실패와 하루 정리는 조용한 시간에
    /// 푸시를 미룬다(거짓). 멈춤과 접속 잠금도 무시한다(결정 2026-11-16 notify 1차: N5 10초 약속,
    /// 밤새 미루면 위험하다).
    pub fn ignores_quiet_hours(self) -> bool {
        !matches!(self, NoticeKind::BackupFailure | NoticeKind::DailySummary)
    }

    /// 푸시 긴급도. 승인 요청 · 멈춤 · 이상 · 접속 잠금은 `High`, 하루 정리는 `High`가 아니다.
    /// 백업 실패는 `Normal`, 하루 정리는 `Low`다.
    pub fn urgency(self) -> Urgency {
        match self {
            NoticeKind::ApprovalRequest
            | NoticeKind::Stall
            | NoticeKind::Anomaly
            | NoticeKind::AccessLock => Urgency::High,
            NoticeKind::BackupFailure => Urgency::Normal,
            NoticeKind::DailySummary => Urgency::Low,
        }
    }
}

impl Urgency {
    /// `Urgency` 머리글 값.
    pub(crate) fn header_value(self) -> &'static str {
        match self {
            Urgency::VeryLow => "very-low",
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::High => "high",
        }
    }
}
