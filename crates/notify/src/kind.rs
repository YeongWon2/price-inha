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
        todo!()
    }

    /// 저장소 이벤트 종류: `notice.` + [`NoticeKind::name`]. 저장소의 종류 이름 규칙에 맞는다.
    pub fn event_kind(self) -> &'static str {
        todo!()
    }

    /// 이름으로 종류를 찾는다. 모르는 이름이면 `None`.
    pub fn from_name(name: &str) -> Option<NoticeKind> {
        let _ = name;
        todo!()
    }

    /// 조용한 시간을 무시하는가: 승인 요청과 이상은 무시한다(참). 백업 실패와 하루 정리는 조용한 시간에
    /// 푸시를 미룬다(거짓). 멈춤과 접속 잠금은 지시서에 정한 말이 없어 시험하지 않는다(구현이 정하고 보고에 적는다).
    pub fn ignores_quiet_hours(self) -> bool {
        todo!()
    }

    /// 푸시 긴급도. 승인 요청 · 멈춤 · 이상 · 접속 잠금은 `High`, 하루 정리는 `High`가 아니다.
    pub fn urgency(self) -> Urgency {
        todo!()
    }
}
