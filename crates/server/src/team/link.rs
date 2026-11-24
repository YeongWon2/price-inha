//! (W5 3차) 팀이 지식 검색 · 사무실과 잇는 곳: 자동 회상(V22), 복기 색인(가린 글만), 검색 도구(T16),
//! 논의 탁자에 모이기(V13), 사무실 채팅 답(V14).

use std::sync::Arc;

use chrono::NaiveDate;
use pi_agent::{DataItem, Role, TimeLimits};
use pi_ai::Redactor;
use pi_journal::ReviewRecord;
use pi_knowledge::{Condition, Knowledge, RecordText, SearchOutput, SearchRequest};
use pi_office::{Activity, Agent, Office};
use pi_store::{KnowledgeKind, KnowledgeSource, Trade, TradeId, UnixMillis};
use rust_decimal::Decimal;
use serde_json::Value;

use super::demo_ai::CHAT_HEAD;
use super::notes::{Records, date_of_input};
use super::review::mistake_word;
use super::talk::MEMBERS;
use super::{Crew, TeamError};
use crate::RECALL_SOURCE_PREFIX;
use crate::reach::Semantic;

/// 자동 회상으로 꺼내는 지난 기록의 가장 많은 수.
const RECALL_MAX: usize = 3;

/// 팀이 쓰는 3차 조립의 부품.
pub(crate) struct CrewLinks {
    pub(crate) knowledge: Arc<Knowledge>,
    pub(crate) office: Option<Arc<Office>>,
    /// (10c 13번) 뜻 검색 설정. 켜져 있으면 뜻 검색은 임베딩을 건 지식 창구로 한다.
    pub(crate) semantic: Option<Arc<Semantic>>,
    /// (16차 3번) 하루 AI 호출 상한에 닿았을 때 사용자에게 알리는 자리.
    pub(crate) ai_capped: Option<Box<dyn Fn() + Send + Sync>>,
}

/// 담당 → 사무실의 담당.
fn office_agent(role: Role) -> Agent {
    match role {
        Role::Scout => Agent::News,
        Role::Strategy => Agent::Strategy,
        Role::Risk => Agent::Risk,
        Role::Review => Agent::Review,
        Role::Trader => Agent::Trader,
        Role::Leader => Agent::Leader,
    }
}

/// 사무실의 담당 → 담당.
pub(crate) fn role_of(agent: Agent) -> Role {
    match agent {
        Agent::News => Role::Scout,
        Agent::Strategy => Role::Strategy,
        Agent::Risk => Role::Risk,
        Agent::Review => Role::Review,
        Agent::Trader => Role::Trader,
        Agent::Leader => Role::Leader,
    }
}

fn refused(why: &str) -> TeamError {
    TeamError::Knowledge(why.to_string())
}

fn text_of<'a>(input: &'a Value, name: &str) -> Option<&'a str> {
    input[name]
        .as_str()
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn decimal_of(input: &Value, name: &str) -> Result<Option<Decimal>, TeamError> {
    match &input[name] {
        Value::Null => Ok(None),
        Value::String(text) => text
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| refused("손익 값을 읽을 수 없다")),
        _ => Err(refused("손익 값은 십진수 글자다")),
    }
}

fn max_of(input: &Value) -> Option<usize> {
    input["max"].as_u64().and_then(|n| usize::try_from(n).ok())
}

/// (17차 5번 · 19차 1번) 논의 가드의 자리 내리기 정리를 만든다. 부르면 작업을 띄워 논의 담당들을 자리로 돌려 놓는다.
pub(super) fn release_table(
    office: Option<Arc<Office>>,
    done: Arc<std::sync::atomic::AtomicBool>,
    _discussing: Arc<std::sync::atomic::AtomicUsize>,
) -> Box<dyn FnOnce() + Send> {
    Box::new(move || {
        if done.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let (Some(office), Ok(runtime)) = (office, tokio::runtime::Handle::try_current()) else {
            return;
        };
        runtime.spawn(async move {
            for role in MEMBERS {
                if let Err(error) = office
                    .set_activity(office_agent(role), Activity::Working)
                    .await
                {
                    tracing::warn!(%error, "사무실 모습을 바꾸지 못했다");
                }
            }
        });
    })
}

impl Crew {
    /// 3차 조립의 부품을 붙인다(한 번만).
    pub(crate) fn link(&self, links: CrewLinks) {
        let _ = self.links.set(links);
    }

    fn now_ms(&self) -> UnixMillis {
        UnixMillis(self.clock.now().timestamp_millis())
    }

    /// 논의에 든 담당의 사무실 모습을 바꾼다(모이기 · 자리로 돌아가기).
    pub(super) async fn at_table(&self, discussing: bool) {
        let Some(office) = self.links.get().and_then(|l| l.office.as_ref()) else {
            return;
        };
        let activity = if discussing {
            Activity::Discussing
        } else {
            Activity::Working
        };
        for role in MEMBERS {
            if let Err(error) = office.set_activity(office_agent(role), activity).await {
                tracing::warn!(%error, "사무실 모습을 바꾸지 못했다");
            }
        }
    }

    /// (17차 5번) 논의 가드가 버려질 때 부르는 정리: 자리 표시를 내린다(`at_table(false)`와 같은 일).
    /// `Drop`에서는 기다릴 수 없어 작업을 띄워 한다. `done`이 이미 참이면(정상으로 끝나 내렸으면) 하지 않는다.
    /// `discussing`은 열려 있는 논의의 수(19차 1번: 띄운 작업이 실제로 돌 때 0이 아니면 내리지 않는다).
    pub(super) fn table_release(
        &self,
        done: Arc<std::sync::atomic::AtomicBool>,
        discussing: Arc<std::sync::atomic::AtomicUsize>,
    ) -> Box<dyn FnOnce() + Send> {
        release_table(
            self.links.get().and_then(|l| l.office.clone()),
            done,
            discussing,
        )
    }

    /// 종목 · 계기로 지난 매매 기록을 자동 회상해 논의 자료로 만든다(V22). 지금 매매는 뺀다.
    ///
    /// 먼저 같은 종목의 팔린 매매를 조건으로 찾고(`recall_by_condition`, 최근 것부터), 모자라면 종목 ·
    /// 계기 낱말로 자동 회상(키워드)한다. 둘 다 바깥 자료의 조각은 넘기지 않는다(M7).
    pub(super) async fn recall(
        &self,
        symbol: &str,
        topic: &str,
        current: TradeId,
    ) -> Vec<DataItem> {
        let Some(links) = self.links.get() else {
            return Vec::new();
        };
        let mut hits = Vec::new();
        match self.trading.sold_records().await {
            Ok(records) => {
                let condition = Condition {
                    symbol: Some(symbol.to_string()),
                    max: Some(RECALL_MAX + 1),
                    ..Condition::default()
                };
                match links
                    .knowledge
                    .recall_by_condition(&condition, &Records::plain(records))
                {
                    Ok(found) => hits.extend(found.results),
                    Err(error) => tracing::warn!(%error, "지난 매매를 조건으로 찾지 못했다"),
                }
            }
            Err(error) => tracing::warn!(%error, "팔린 매매를 읽지 못했다"),
        }
        let request = SearchRequest {
            text: format!("{symbol} {topic}"),
            kinds: vec![KnowledgeKind::Trade, KnowledgeKind::Review],
            sources: Vec::new(),
            auto_recall: true,
            max: Some(RECALL_MAX * 2),
        };
        match links.knowledge.search(&request, self.now_ms()) {
            Ok(found) => hits.extend(found.results),
            Err(error) => tracing::warn!(%error, "지난 매매를 찾지 못했다"),
        }
        let mut items: Vec<DataItem> = Vec::new();
        for hit in hits {
            let name = format!("{RECALL_SOURCE_PREFIX}{}", hit.record_ref);
            if hit.record_ref == current.0.to_string() || items.iter().any(|i| i.name == name) {
                continue;
            }
            items.push(DataItem {
                name,
                content: Some(hit.snippet),
                known_at: self.now_ms(),
            });
            if items.len() == RECALL_MAX {
                break;
            }
        }
        items
    }

    /// 복기한 매매의 기록(종류 `trade`)과 복기(종류 `review`)를 가린 글로 색인한다.
    pub(super) fn index_review(&self, trade: &Trade, record: &ReviewRecord) {
        let Some(links) = self.links.get() else {
            return;
        };
        let id = trade.id.0.to_string();
        let mistakes: Vec<String> = record.mistakes.iter().map(mistake_word).collect();
        let texts = [
            (
                KnowledgeKind::Trade,
                format!(
                    "매매 {id} · 종목 {}\n{}",
                    trade.symbol,
                    pi_journal::plain_summary(trade)
                ),
            ),
            (
                KnowledgeKind::Review,
                format!(
                    "복기 · 매매 {id} · 종목 {}\n실수: {}\n{}",
                    trade.symbol,
                    mistakes.join(", "),
                    record.note
                ),
            ),
        ];
        let mut records: Vec<RecordText> = texts
            .into_iter()
            .map(|(kind, text)| RecordText {
                kind,
                source: KnowledgeSource::Team,
                record_ref: id.clone(),
                text,
                at: record.at,
            })
            .collect();
        // (10c 12번) 실수가 있으면 실수 기록(종류 mistake · 등급 team · 기록 키 = 매매 번호). 없으면 만들지 않는다.
        records.extend(RecordText::mistakes(
            &id,
            &trade.symbol,
            &mistakes,
            &record.note,
            record.at,
        ));
        for mut record in records {
            record.text = self.redactor.redact(&record.text);
            if let Err(error) = links.knowledge.index(record) {
                tracing::warn!(%error, "기록을 색인하지 못했다");
            }
        }
    }

    /// 검색 도구의 몸통([`Crew::search_knowledge`]).
    pub(super) async fn search_tool(&self, input: &Value) -> Result<SearchOutput, TeamError> {
        let links = self
            .links
            .get()
            .ok_or_else(|| refused("지식 검색을 잇지 않았다"))?;
        let knowledge = &links.knowledge;
        let failed = |error: pi_knowledge::Error| refused(&error.to_string());
        match input["mode"].as_str() {
            Some("keyword") => {
                let query = text_of(input, "query").ok_or_else(|| refused("질의가 비었다"))?;
                let mut request = SearchRequest::keywords(query);
                request.max = max_of(input);
                knowledge.search(&request, self.now_ms()).map_err(failed)
            }
            Some("condition") => {
                let condition = Condition {
                    symbol: text_of(input, "symbol").map(str::to_string),
                    max_pnl: decimal_of(input, "max_pnl")?,
                    min_pnl: decimal_of(input, "min_pnl")?,
                    max: max_of(input),
                    mistake: text_of(input, "mistake").map(str::to_string),
                    ..Condition::default()
                };
                // 날짜는 그 시장 달력의 날짜라 시각 경계를 짓지 않고 매매의 시장 날짜로 먼저 거른다.
                let (from, to) = (date_of_input(input, "from")?, date_of_input(input, "to")?);
                let mut records = self.catalog().await?;
                self.within_dates(&mut records, from, to);
                knowledge
                    .search_by_condition(&condition, &records)
                    .map_err(failed)
            }
            Some("semantic") => {
                let query = text_of(input, "query").ok_or_else(|| refused("질의가 비었다"))?;
                let embedded = links.semantic.as_ref().and_then(|s| s.knowledge());
                embedded
                    .as_ref()
                    .unwrap_or(knowledge)
                    .semantic_search(query, self.now_ms())
                    .await
                    .map_err(failed)
            }
            _ => Err(refused(
                "모드는 keyword · condition · semantic 가운데 하나다",
            )),
        }
    }

    /// 사무실 채팅에서 그 담당에게 물은 답(시간 제한은 채팅 것: 무응답 180초 · 전체 600초).
    pub(crate) async fn chat(&self, role: Role, text: &str) -> Result<String, TeamError> {
        let input = format!("{CHAT_HEAD} {}", self.redactor.redact(text));
        let title = format!("chat-{}", role.id());
        let outcome = self
            .team
            .runner()
            .run_turn(self.role(role), TimeLimits::CHAT, &title, &input)
            .await
            .map_err(|error| TeamError::Agent(self.redactor.redact(&error.to_string())))?;
        Ok(outcome.text)
    }

    /// 그 시장 날짜에 남긴 복기들.
    pub(crate) fn reviews_on(&self, date: NaiveDate) -> Result<Vec<ReviewRecord>, TeamError> {
        let reviews = self
            .journal
            .reviews()
            .map_err(|error| TeamError::Journal(error.to_string()))?;
        Ok(reviews
            .into_iter()
            .filter(|r| {
                let at = chrono::DateTime::from_timestamp_millis(r.at.0).unwrap_or_default();
                self.trading.date_of(at) == date
            })
            .collect())
    }
}
