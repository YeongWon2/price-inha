//! 매매 논의(T9 · T10): 새 공시 보기와 종목 하나의 논의.

use pi_agent::{DiscussionLimits, DiscussionRequest, Role};
use pi_data::{FactValue, Fetched};
use pi_market::Symbol;
use pi_store::{AccountKind, NewTrade, StoredFact, TradeId, UnixMillis};

use super::facts::{self, Gathered};
use super::{Crew, Discussed, TRADE_ROOM_PREFIX};
use crate::TeamError;

/// 논의에 나오는 담당(명단 순서). 매매 담당은 판단 뒤 주문 안 턴에만 나온다.
const MEMBERS: [Role; 4] = [Role::Scout, Role::Strategy, Role::Risk, Role::Leader];

impl Crew {
    fn now_millis(&self) -> UnixMillis {
        UnixMillis(self.clock.now().timestamp_millis())
    }

    /// 지켜보는 종목마다 아직 논의하지 않은 공시가 있으면 논의한다.
    pub(super) async fn scan_watched(&self) -> Result<Vec<Discussed>, TeamError> {
        let mut discussed = Vec::new();
        for symbol in self.desk.symbols() {
            let Some(topic) = self.new_disclosure(symbol.as_str()).await? else {
                continue;
            };
            match self.discuss_symbol(symbol.as_str(), &topic).await {
                Ok(done) => discussed.push(done),
                Err(error) => tracing::warn!(%error, "공시 논의를 마치지 못했다"),
            }
        }
        Ok(discussed)
    }

    /// 그 종목의 공시 가운데 저장소에 아직 없는 것(접수번호 기준)의 계기 글.
    async fn new_disclosure(&self, symbol: &str) -> Result<Option<String>, TeamError> {
        let now = self.clock.now();
        let today = self.trading.date_of(now);
        let Some(request) = facts::disclosures(&self.wiring, symbol, today) else {
            return Ok(None);
        };
        let found = match self.data.fetch(&request).await {
            Ok(Fetched::Facts(found)) => found,
            Ok(Fetched::Missing(_)) => return Ok(None),
            Err(error) => {
                tracing::warn!(%error, "공시를 받지 못했다");
                return Ok(None);
            }
        };
        let mut seen: Vec<StoredFact> = Vec::new();
        for subject in found.iter().map(|fact| fact.subject.as_str()) {
            seen.extend(self.store.facts_known_by(subject, self.now_millis())?);
        }
        let fresh = found.iter().find(|fact| {
            !seen.iter().any(|old| {
                old.kind == facts::kind_name(fact.kind) && old.reference == fact.reference
            })
        });
        Ok(fresh.map(|fact| match &fact.value {
            FactValue::Text(title) => format!("새 공시: {title}"),
            FactValue::Number(_) => "새 공시".to_string(),
        }))
    }

    /// 종목 하나를 논의한다(공개 [`Crew::discuss`]).
    pub(super) async fn discuss_symbol(
        &self,
        symbol: &str,
        topic: &str,
    ) -> Result<Discussed, TeamError> {
        if Symbol::parse(symbol).is_err() {
            return Err(TeamError::UnknownSymbol(symbol.to_string()));
        }
        let now = self.clock.now();
        let trade = self.store.create_trade(NewTrade {
            symbol: symbol.to_string(),
            account: AccountKind::Paper,
            at: self.now_millis(),
        })?;
        let today = self.trading.date_of(now);
        let gathered = facts::gather(
            self.data.as_ref(),
            &self.store,
            &self.wiring,
            symbol,
            (today, now),
        )
        .await?;
        let price = self
            .trading
            .last_quote(symbol)
            .map(|quote| format!(" · 현재가 {}", quote.price.normalize()))
            .unwrap_or_default();
        let request = DiscussionRequest {
            room: format!("{TRADE_ROOM_PREFIX}{}", trade.id.0),
            trade: trade.id,
            topic: format!("{topic} · 종목 {symbol}{price}"),
            members: MEMBERS.to_vec(),
            nominated: vec![Role::Scout],
            data: gathered.items.clone(),
            consensus_required: self.wiring.consensus_required,
            limits: DiscussionLimits::default(),
        };
        let result = self.team.discuss(request).await;
        self.attach(trade.id, &gathered)?;
        let outcome = result.map_err(|error| TeamError::Agent(error.to_string()))?;
        Ok(Discussed {
            trade: trade.id,
            outcome,
            placed: self.tools.take(trade.id),
        })
    }

    /// 판단된 매매에 받은 자료를 붙인다(판단 시각보다 늦게 안 것은 뺀다, V36).
    fn attach(&self, trade: TradeId, gathered: &Gathered) -> Result<(), TeamError> {
        let Some(decided) = self.store.get_trade(trade)?.decided_at else {
            return Ok(());
        };
        let ids: Vec<_> = gathered
            .facts
            .iter()
            .filter(|(_, known)| *known <= decided)
            .map(|(id, _)| *id)
            .collect();
        if !ids.is_empty() {
            self.store.attach_trade_facts(trade, &ids)?;
        }
        Ok(())
    }
}
