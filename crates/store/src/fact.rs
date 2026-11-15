//! 자료(Fact)와 알게 된 시각(10차 3번, C11 · V36).
//!
//! - `pi-data`의 `Fact`를 조립에서 그대로 옮겨 담을 수 있게 칸을 맞춘다(store는 data에 기대지 않는다).
//!   수 값은 부동소수점이 아니라 십진 글자 그대로 둔다.
//! - 자료는 쌓기만 한다. 지우거나 고치는 API는 없다.
//! - 매매에 붙인 자료(매매 근거의 자료)는 판단 시각보다 늦게 알게 된 것이 0건이다(V36).
//!   표가 스스로 지킨다(어떤 SQL로도): 붙인 자료 표는 `trade_facts(trade_id, fact_id)`이고,
//!   판단 시각이 없는 매매나 판단 시각보다 늦게 알게 된 자료를 가리키는 줄은 들어가지 못한다.
//!   자료 표는 `facts`이고 알게 된 시각 칸은 `known_at`(밀리초 정수)이다. 저장된 자료의 줄은
//!   고치거나 지울 수 없다(알게 된 시각을 뒤로 미뤄 V36을 깨지 못한다).

use crate::model::{TradeId, UnixMillis};
use crate::{Store, StoreError};

/// 자료 번호. 저장소가 붙인다(1부터).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactId(pub i64);

/// 자료 값.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactValue {
    /// 수. 십진 글자 그대로(예: `"-12.340"`) 저장하고 꺼낸다. 한 글자도 바뀌지 않는다.
    Number(String),
    /// 글(공시 보고서명 등).
    Text(String),
}

/// 저장할 자료 하나(C11 성공: 자료와 알게 된 시각).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFact {
    /// 종류 이름(예: `disclosure`, `financials`).
    pub kind: String,
    /// 무엇에 대한 자료인가(종목 · 고유번호 · 계열 id).
    pub subject: String,
    /// 이름.
    pub label: String,
    /// 자료가 가리키는 시점. 없을 수 있다.
    pub period: Option<String>,
    /// 값.
    pub value: FactValue,
    /// 단위. 없을 수 있다.
    pub unit: Option<String>,
    /// 출처 안의 참조 번호. 없을 수 있다.
    pub reference: Option<String>,
    /// 출처 이름.
    pub source_name: String,
    /// 요청 주소(키 자리는 가려진 것).
    pub source_url: String,
    /// 공표일(`YYYY-MM-DD`). 없을 수 있다.
    pub published_on: Option<String>,
    /// 알게 된 시각(응답을 받은 시각).
    pub known_at: UnixMillis,
}

/// 저장된 자료.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFact {
    /// 번호.
    pub id: FactId,
    /// 종류 이름.
    pub kind: String,
    /// 무엇에 대한 자료인가.
    pub subject: String,
    /// 이름.
    pub label: String,
    /// 시점.
    pub period: Option<String>,
    /// 값.
    pub value: FactValue,
    /// 단위.
    pub unit: Option<String>,
    /// 참조 번호.
    pub reference: Option<String>,
    /// 출처 이름.
    pub source_name: String,
    /// 요청 주소.
    pub source_url: String,
    /// 공표일.
    pub published_on: Option<String>,
    /// 알게 된 시각.
    pub known_at: UnixMillis,
}

impl Store {
    /// 자료 여러 개를 한 묶음으로 저장하고 준 순서대로 번호를 붙여 돌려준다. 칸은 넣은 그대로다.
    pub fn save_facts(&self, facts: &[NewFact]) -> Result<Vec<StoredFact>, StoreError> {
        todo!()
    }

    /// 자료 한 건. 없으면 [`StoreError::FactNotFound`].
    pub fn get_fact(&self, id: FactId) -> Result<StoredFact, StoreError> {
        todo!()
    }

    /// 그 대상(`subject`)의 자료 가운데 판단 시각 `decision_at`에 이미 알 수 있던 것
    /// (`known_at <= decision_at`, 같은 시각은 든다)만 준다. 번호 순서다(V36).
    pub fn facts_known_by(
        &self,
        subject: &str,
        decision_at: UnixMillis,
    ) -> Result<Vec<StoredFact>, StoreError> {
        todo!()
    }

    /// 판단한 매매에 근거 자료를 붙인다(V36). 하나라도 거부되면 아무것도 붙이지 않는다.
    ///
    /// - 매매가 없으면 [`StoreError::TradeNotFound`], 자료가 없으면 [`StoreError::FactNotFound`].
    /// - 판단 시각이 없는(판단 전) 매매는 [`StoreError::TradeNotJudged`].
    /// - 자료의 알게 된 시각이 매매의 판단 시각보다 늦으면 [`StoreError::FactAfterDecision`]
    ///   (준 순서에서 처음 것). 같은 시각은 된다.
    /// - 이미 붙은 자료를 다시 주면 한 번만 남는다.
    /// - 되면 그 매매에 붙은 자료 전체([`Store::trade_facts`]와 같은 것)를 돌려준다.
    pub fn attach_trade_facts(
        &self,
        trade: TradeId,
        facts: &[FactId],
    ) -> Result<Vec<StoredFact>, StoreError> {
        todo!()
    }

    /// 매매에 붙은 자료들. 처음 붙인 순서다. 매매가 없으면 [`StoreError::TradeNotFound`].
    pub fn trade_facts(&self, trade: TradeId) -> Result<Vec<StoredFact>, StoreError> {
        todo!()
    }
}
