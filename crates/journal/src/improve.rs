//! 규칙 개선안(M4 · V20 · V21 · E28 · E35).

use pi_store::Proposal;
use pi_template::RiskParams;
use serde::{Deserialize, Serialize};

use crate::{Error, Mistake};

/// 개선안 내용: 실수 종류와 바꾸기 전 · 후 값(RiskParams 차이). 저장소 개선안의 `content`에 그대로 들어간다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Improvement {
    /// 어떤 실수 때문인가.
    pub mistake: Mistake,
    /// 바꾸기 전 값.
    pub before: RiskParams,
    /// 바꾼 뒤 값.
    pub after: RiskParams,
}

impl Improvement {
    /// 저장소 개선안의 `content`를 읽는다. 모양이 다르면 [`Error::BadProposal`].
    pub fn from_proposal(proposal: &Proposal) -> Result<Improvement, Error> {
        todo!()
    }
}

/// 개선안이 위험을 키우는가(M4): 한도나 손절을 넓히는 변경이면 참.
///
/// 키우는 것으로 본다: 1회 위험 · 일일 손실 · 최대 낙폭 · 종목 · 확신 종목 · 업종 비중 · 자동 주문 한도의 상향,
/// 손절 폭(고정 손실 위 끝)의 상향, 최대 동시 보유 수 · 일일 최대 매매 횟수의 상향.
/// 좁히거나 같거나 다른 칸만 바뀌면 거짓이다.
pub fn increases_risk(before: &RiskParams, after: &RiskParams) -> bool {
    todo!()
}

/// 승인 · 거절 · 냉각 해제의 결과: 개선안의 지금 모습과 그 뒤 적용되는 규칙.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleOutcome {
    /// 개선안(저장소에서 읽은 지금 모습).
    pub proposal: Proposal,
    /// 이 뒤 적용되는 규칙. 반영됨이면 개선안의 `after`, 아니면(냉각중 · 거절됨) 받은 `current` 그대로다.
    pub rules: RiskParams,
}
