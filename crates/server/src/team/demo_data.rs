//! (24차 4번, E56) 실행 파일의 시연 조립이 쓰는 자료 어댑터: 바깥(DART · ECOS · FRED)에 묻지 않고, 논의가
//! 받을 자료 자리마다 "시연" 값을 돌려준다(키가 등록돼 있어도 묻지 않는다).
//!
//! 키 없이 묻지 않기만 하면 논의의 자료가 모두 비어(내용 없음) 팀이 늘 지켜보기(`missing_data`, E32)로 끝나 시연에서
//! 논의가 열리지 않는다. 그래서 자료 자리마다 "시연용 가짜 값"이라고 적힌 글 하나를 준다. 공시는 "공시 없음"이다
//! (가짜 공시는 [`super::demo_feed`]가 깨우기로만 넣는다). 실제 출처 주소는 쓰지 않는다.

use std::sync::Arc;

use async_trait::async_trait;
use pi_data::{
    DataAdapter, DataError, Fact, FactKind, FactRequest, FactValue, Fetched, Source, SourceName,
};
use pi_market::Clock;

use crate::DEMO_DISCLOSURE_MARK;

/// 시연 자료의 출처 주소 자리(실제 주소가 아니다).
const DEMO_URL: &str = "시연(바깥에 묻지 않음)";

/// 시연 자료 어댑터.
pub(super) struct DemoData {
    clock: Arc<dyn Clock>,
}

impl DemoData {
    pub(super) fn new(clock: Arc<dyn Clock>) -> DemoData {
        DemoData { clock }
    }
}

/// 그 종류의 이름 · 출처.
fn label_of(kind: FactKind) -> (&'static str, SourceName) {
    match kind {
        FactKind::Financials => ("재무", SourceName::Dart),
        FactKind::KoreaRate => ("국내 금리", SourceName::Ecos),
        FactKind::KoreaFx => ("환율", SourceName::Ecos),
        FactKind::UsRate => ("미국 금리", SourceName::Fred),
        _ => ("공시", SourceName::Dart),
    }
}

#[async_trait]
impl DataAdapter for DemoData {
    async fn fetch(&self, request: &FactRequest) -> Result<Fetched, DataError> {
        let kind = request.kind();
        if kind == FactKind::Disclosure {
            return Ok(Fetched::Missing(kind));
        }
        let (label, name) = label_of(kind);
        let mark = DEMO_DISCLOSURE_MARK;
        Ok(Fetched::Facts(vec![Fact {
            kind,
            subject: format!("{mark}-{label}"),
            label: format!("{mark} {label}"),
            period: None,
            value: FactValue::Text(format!("{mark}용 가짜 값(실제 자료가 아닙니다)")),
            unit: None,
            reference: None,
            source: Source {
                name,
                url: DEMO_URL.to_string(),
            },
            published_on: None,
            known_at: self.clock.now(),
        }]))
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use pi_market::SystemClock;

    use super::*;

    #[tokio::test]
    async fn 거시_자료_자리마다_시연_글_하나를_준다() {
        let data = DemoData::new(Arc::new(SystemClock));
        let date = NaiveDate::from_ymd_opt(2026, 11, 21).unwrap_or_default();
        for request in super::super::facts::macros(&crate::TeamWiring::official(), date) {
            let fetched = data.fetch(&request).await;
            let facts = match fetched {
                Ok(Fetched::Facts(facts)) => facts,
                _ => Vec::new(),
            };
            assert_eq!(facts.len(), 1, "자료 자리마다 시연 글 하나");
            assert!(
                matches!(&facts[0].value, FactValue::Text(text) if text.contains(DEMO_DISCLOSURE_MARK))
            );
            assert!(
                !facts[0].source.url.starts_with("http"),
                "실제 주소가 아니다"
            );
        }
    }
}
