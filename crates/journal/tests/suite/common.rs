//! 시험 도구: 임시 폴더의 실제 저장소, 매매 만들기, 승인 만들기.

use std::path::PathBuf;

use pi_journal::Journal;
use pi_store::{
    AccountKind, Amount, ApprovalId, ApprovalStatus, ApprovalUse, Judgment, NewApprovalRequest,
    NewTrade, Rationale, RoleScore, SourceRef, Store, TradeId, TradeState, UnixMillis,
};
use pi_template::{RiskParams, Style, preset};
use tempfile::TempDir;

/// 시험의 시작 시각(2027-01-15 17:00 한국 시간).
pub const T0: i64 = 1_800_000_000_000;
/// 1분(밀리초).
pub const MIN: i64 = 60_000;
/// 하루(밀리초).
pub const DAY: i64 = 24 * 60 * MIN;

/// 시각.
pub fn t(ms: i64) -> UnixMillis {
    UnixMillis(ms)
}

/// 임시 폴더의 저장소와 그 위의 복기 창구.
pub struct Env {
    pub dir: TempDir,
    pub path: PathBuf,
    pub store: Store,
    pub journal: Journal,
}

impl Env {
    pub fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.db");
        let store = Store::open(&path).unwrap();
        let journal = Journal::new(store.clone());
        Env {
            dir,
            path,
            store,
            journal,
        }
    }

    /// 같은 파일을 새 손잡이로 다시 연다(앞 손잡이는 버린다).
    pub fn reopen(self) -> Env {
        let Env { dir, path, .. } = self;
        let store = Store::open(&path).unwrap();
        let journal = Journal::new(store.clone());
        Env {
            dir,
            path,
            store,
            journal,
        }
    }

    /// 근거가 다 찬 매매를 지정한 상태까지 보낸다. 모든 전이는 `at`에 일어난다.
    pub fn trade_in(&self, symbol: &str, state: TradeState, at: i64) -> TradeId {
        let trade = self
            .store
            .create_trade(NewTrade {
                symbol: symbol.to_string(),
                account: AccountKind::Paper,
                at: t(at),
            })
            .unwrap();
        let id = trade.id;
        if state == TradeState::Discussing {
            return id;
        }
        self.store.judge_trade(id, judgment(at)).unwrap();
        for step in [
            TradeState::Ordered,
            TradeState::Holding,
            TradeState::Sold,
            TradeState::Reviewed,
        ] {
            if state == TradeState::Judged {
                break;
            }
            self.store.transition_trade(id, step, t(at)).unwrap();
            if step == state {
                break;
            }
        }
        id
    }

    /// 팔린 매매.
    pub fn sold(&self, symbol: &str, at: i64) -> TradeId {
        self.trade_in(symbol, TradeState::Sold, at)
    }

    /// 승인된 요청을 만들어 쓸 수 있는 형태로 준다. 지문은 `fingerprint`다.
    pub fn approved(&self, fingerprint: &str, at: i64) -> ApprovalUse {
        let request = self
            .store
            .create_approval(NewApprovalRequest {
                fingerprint: fingerprint.to_string(),
                summary: serde_json::json!({"kind": "rule_change"}),
                created_at: t(at),
                expires_at: t(at + DAY),
            })
            .unwrap();
        self.store
            .resolve_approval(request.id, ApprovalStatus::Approved, t(at + MIN))
            .unwrap();
        ApprovalUse {
            id: request.id,
            fingerprint: fingerprint.to_string(),
        }
    }

    /// 기다림 상태로 둔(아직 답이 없는) 승인 번호.
    pub fn pending_approval(&self, at: i64) -> ApprovalId {
        self.store
            .create_approval(NewApprovalRequest {
                fingerprint: "fp-pending".to_string(),
                summary: serde_json::json!({}),
                created_at: t(at),
                expires_at: t(at + DAY),
            })
            .unwrap()
            .id
    }
}

/// 근거 여섯 칸이 모두 찬 판단.
pub fn judgment(at: i64) -> Judgment {
    Judgment {
        decided_at: t(at),
        rationale: Rationale {
            hypothesis: Some("실적 발표 뒤 눌림에서 반등한다".to_string()),
            trigger: Some("거래량 두 배 돌파".to_string()),
            entry: Some(Amount(70_000)),
            stop: Some(Amount(68_000)),
            target: Some(Amount(75_000)),
            expected_value: Some(Amount(1_500)),
        },
        sources: vec![SourceRef {
            name: "분기 실적 공시".to_string(),
            known_at: t(T0),
        }],
        scores: vec![
            RoleScore {
                role: "분석가".to_string(),
                score: 7,
                reason: "이익이 늘었다".to_string(),
            },
            RoleScore {
                role: "위험 담당".to_string(),
                score: 4,
                reason: "변동폭이 크다".to_string(),
            },
        ],
        consensus_score: 6,
    }
}

/// 일일 손실 한도 1.5%인 단타 기본 규칙.
pub fn base_params() -> RiskParams {
    preset(Style::DayTrade)
}
