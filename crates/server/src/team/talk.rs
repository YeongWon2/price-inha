//! 매매 논의(T9 · T10): 새 공시 보기와 종목 하나의 논의.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use pi_agent::{DiscussionLimits, DiscussionOutcome, DiscussionRequest, Role};
use pi_data::{FactValue, Fetched};
use pi_market::Symbol;
use pi_store::{
    AccountKind, NewRoomEvent, NewTrade, RoomActorKind, StoredFact, TradeId, UnixMillis,
};

use super::facts::{self, Gathered};
use super::{Crew, Discussed, TRADE_ROOM_PREFIX};
use crate::TeamError;

/// 논의에 나오는 담당(명단 순서). 매매 담당은 판단 뒤 주문 안 턴에만 나온다.
pub(super) const MEMBERS: [Role; 4] = [Role::Scout, Role::Strategy, Role::Risk, Role::Leader];

/// 열려 있는 논의의 수를 세는 가드: 만들면 수가 하나 늘고, 버려지면(끝 · panic · 취소) 하나 준다.
/// 뜻 검색 다시 만들기는 이 수가 0일 때만 돈다([`crate::Crew::discussing`]). `fetch_add` → 논의 → `fetch_sub`를
/// 손으로 짝지으면 논의가 panic · 취소됐을 때 수가 영영 줄지 않아 다시 만들기가 쉬지 않는다.
pub(super) struct DiscussingGuard {
    /// 세는 수(지켜보는 쪽과 함께 쓴다).
    count: Arc<AtomicUsize>,
    /// 버려질 때 한 번 부르는 정리(사무실 자리 표시 내리기 등).
    release: Option<Box<dyn FnOnce() + Send>>,
}

impl DiscussingGuard {
    /// 논의 하나를 열었다고 세고, 버려지면 되돌리는 가드를 돌려준다.
    pub(super) fn enter(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self {
            count: Arc::clone(count),
            release: None,
        }
    }

    /// (17차 5번) [`DiscussingGuard::enter`]에 더해, 버려질 때(끝 · panic · 취소) `release`를 한 번 부른다.
    /// 논의 도중 취소돼도 사무실 자리 표시(논의 탁자 → 자리로 돌아가기)가 내려가게 하는 자리다.
    pub(super) fn enter_with(count: &Arc<AtomicUsize>, release: Box<dyn FnOnce() + Send>) -> Self {
        let mut guard = Self::enter(count);
        guard.release = Some(release);
        guard
    }
}

impl Drop for DiscussingGuard {
    /// 센 것을 되돌리고 정리를 한 번 부른다(panic · 취소로 버려져도).
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

/// 그 매매의 논의 방 이름.
fn request_room(trade: TradeId) -> String {
    format!("{TRADE_ROOM_PREFIX}{}", trade.0)
}

impl Crew {
    fn now_millis(&self) -> UnixMillis {
        UnixMillis(self.clock.now().timestamp_millis())
    }

    /// (16차 3번) 하루 AI 호출 상한이 켜져 있으면 논의 한 번을 한 번으로 센다(한국 시각 날짜별, 저장).
    /// 상한에 닿았으면 그날 남은 논의를 쉬고(오류) 사용자에게 그날 한 번 알린다. 손절 등은 AI를 부르지 않아 상관없다.
    pub(super) fn take_ai_call(&self) -> Result<(), TeamError> {
        let limit = self
            .wiring
            .ai_daily_limit
            .or_else(|| self.desk.saved_ai_limit());
        let Some(limit) = limit else {
            return Ok(());
        };
        let now = self.clock.now();
        if self.ai_cap.take_within(now, limit) {
            return Ok(());
        }
        let today = super::quota::kst_date(now);
        let first_today = {
            let mut told = self
                .ai_cap_told
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let first = *told != Some(today);
            *told = Some(today);
            first
        };
        if first_today {
            tracing::warn!("오늘 AI 호출 상한에 닿아 논의를 쉽니다");
            if let Some(notify) = self.links.get().and_then(|links| links.ai_capped.as_ref()) {
                notify();
            }
        }
        Err(TeamError::AiLimit)
    }

    /// (22차 1번) 논의가 도는 동안 그 방의 담당 글을 사무실 팀 대화로 낸다(사무실이 없으면 없음).
    fn follow_talk(&self, trade: TradeId, symbol: &str) -> Option<crate::reach::Following> {
        let talk = self.links.get().and_then(|links| links.talk.as_ref())?;
        Some(talk.follow(&request_room(trade), symbol, self.trading.market()))
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
    pub(super) async fn new_disclosure(&self, symbol: &str) -> Result<Option<String>, TeamError> {
        let now = self.clock.now();
        let today = self.trading.date_of(now);
        let Some(request) = facts::disclosures(&self.wiring, symbol, today) else {
            return Ok(None);
        };
        let found = match self.data.fetch(&request).await {
            Ok(Fetched::Facts(found)) => found,
            Ok(Fetched::Missing(_)) => return Ok(None),
            // 하루 상한에 닿은 것은 "공시 없음(한도)"이다(DART 어댑터가 그날 한 번 남긴다).
            Err(error) if super::quota::over_limit(&error) => return Ok(None),
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
        self.take_ai_call()?;
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
        let mut gathered = gathered;
        let news = self.news.gather(symbol, now).await?;
        if let Some(news) = &news {
            gathered.items.push(news.item.clone());
            gathered.facts.extend(news.facts.iter().copied());
            gathered.asked += 1;
            gathered.failed += u32::from(news.failed);
        }
        self.index_external(&gathered, news.as_ref());
        self.trading.note_data(!gathered.all_failed());
        let price = self
            .trading
            .last_quote(symbol)
            .map(|quote| format!(" · 현재가 {}", quote.price.normalize()))
            .unwrap_or_default();
        let mut data = gathered.items.clone();
        data.extend(self.recall(symbol, topic, trade.id).await);
        let request = DiscussionRequest {
            room: request_room(trade.id),
            trade: trade.id,
            topic: format!("{topic} · 종목 {symbol}{price}"),
            members: MEMBERS.to_vec(),
            nominated: vec![Role::Scout],
            data,
            consensus_required: self.wiring.consensus_required,
            limits: DiscussionLimits::default(),
        };
        let following = self.follow_talk(trade.id, symbol);
        let result = self.discuss_at_table(request).await;
        if let Some(following) = following {
            following.finish().await;
        }
        if let Some(news) = news.filter(|news| news.dropped > 0) {
            self.log_dropped(&request_room(trade.id), news.dropped);
        }
        self.attach(trade.id, &gathered)?;
        let outcome = result.map_err(|error| TeamError::Agent(error.to_string()))?;
        Ok(Discussed {
            trade: trade.id,
            outcome,
            placed: self.tools.take(trade.id),
        })
    }

    /// 논의를 미팅룸 자리 표시와 함께 돈다. (33차 3번) 공지는 논의 옵션으로 넘긴다.
    async fn discuss_at_table(
        &self,
        request: DiscussionRequest,
    ) -> Result<DiscussionOutcome, pi_agent::Error> {
        // (17차 5번) 취소 · panic에도 자리 표시가 내려가게 가드의 정리로 건다.
        let lowered = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let _discussing = DiscussingGuard::enter_with(
            &self.discussing,
            self.table_release(Arc::clone(&lowered), Arc::clone(&self.discussing)),
        );
        self.at_table(true).await;
        let result = self
            .team
            .discuss_with(request, self.discussion_options())
            .await;
        if !lowered.swap(true, Ordering::SeqCst) {
            self.at_table(false).await;
        }
        result
    }

    /// (10b 9번) 링크가 이상해 버린 뉴스 기사 수를 그 매매의 방 로그에 남긴다(결정 2026-11-21 16번).
    fn log_dropped(&self, room: &str, dropped: usize) {
        let event = NewRoomEvent {
            room: room.to_string(),
            event_id: format!("news-dropped-{room}"),
            kind: "room.activity".to_string(),
            actor_kind: RoomActorKind::Gateway,
            actor_id: "server".to_string(),
            payload: serde_json::json!({
                "news_dropped": dropped,
                "text": format!("링크가 이상한 뉴스 기사 {dropped}건을 버렸습니다"),
            }),
            at: self.now_millis(),
        };
        if let Err(error) = self.store.append_room_event(event) {
            tracing::warn!(%error, "버린 뉴스 기사 수를 방 로그에 남기지 못했다");
        }
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

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::time::Duration;

    use super::*;

    fn count() -> Arc<AtomicUsize> {
        Arc::new(AtomicUsize::new(0))
    }

    // 16차 5번: N2가 약하게 하면 안 되는 시험들이다(가드 하나로 끝 · 둘 · panic · 취소를 모두 본다).
    #[test]
    fn 가드는_만들면_수가_하나_늘고_버려지면_하나_준다() {
        let count = count();
        let guard = DiscussingGuard::enter(&count);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(guard);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn 가드가_여럿이면_버려진_만큼만_준다() {
        let count = count();
        let first = DiscussingGuard::enter(&count);
        let second = DiscussingGuard::enter(&count);
        assert_eq!(count.load(Ordering::SeqCst), 2);
        drop(first);
        assert_eq!(count.load(Ordering::SeqCst), 1);
        drop(second);
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn 가드를_쥔_채_panic해도_수가_줄어든다() {
        let count = count();
        let reached = std::sync::atomic::AtomicBool::new(false);
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let _guard = DiscussingGuard::enter(&count);
            assert_eq!(count.load(Ordering::SeqCst), 1);
            reached.store(true, Ordering::SeqCst);
            panic!("논의 도중 panic");
        }));
        assert!(outcome.is_err());
        assert!(
            reached.load(Ordering::SeqCst),
            "가드를 만든 뒤 panic까지 갔어야 한다"
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "panic으로 풀려도 수가 돌아온다"
        );
    }

    #[tokio::test]
    async fn 가드를_쥔_채_기다리던_작업을_취소하면_수가_줄어든다() {
        let count = count();
        let held = Arc::clone(&count);
        let task = tokio::spawn(async move {
            let _guard = DiscussingGuard::enter(&held);
            // 끝나지 않는 논의(AI 답을 영영 기다린다).
            std::future::pending::<()>().await;
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while count.load(Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < deadline,
                "작업이 가드를 쥐어야 한다"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        task.abort();
        let _cancelled = task.await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "취소로 버려져도 수가 돌아온다"
        );
    }

    // 17차 5번: N2가 약하게 하면 안 되는 시험들이다(끝 · panic · 취소 모두에서 자리 표시를 내리는 일이 정확히 한 번 불린다).
    fn releasing() -> (Arc<AtomicUsize>, Arc<AtomicUsize>, DiscussingGuard) {
        let count = count();
        let released = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&released);
        let guard = DiscussingGuard::enter_with(
            &count,
            Box::new(move || {
                seen.fetch_add(1, Ordering::SeqCst);
            }),
        );
        (count, released, guard)
    }

    #[test]
    fn 자리_표시_정리는_가드가_버려질_때_한_번_불리고_수도_함께_준다() {
        let (count, released, guard) = releasing();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            released.load(Ordering::SeqCst),
            0,
            "버려지기 전에는 부르지 않는다"
        );
        drop(guard);
        assert_eq!(released.load(Ordering::SeqCst), 1, "끝나면 한 번 부른다");
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn 가드를_쥔_채_panic해도_자리_표시_정리가_불린다() {
        let (count, released, guard) = releasing();
        let outcome = catch_unwind(AssertUnwindSafe(move || {
            let _guard = guard;
            panic!("논의 도중 panic");
        }));
        assert!(outcome.is_err());
        assert_eq!(
            released.load(Ordering::SeqCst),
            1,
            "panic으로 풀려도 한 번 부른다"
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn 가드를_쥔_채_기다리던_작업을_취소해도_자리_표시_정리가_불린다() {
        let (count, released, guard) = releasing();
        let task = tokio::spawn(async move {
            let _guard = guard;
            // 끝나지 않는 논의(AI 답을 영영 기다린다).
            std::future::pending::<()>().await;
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(
            released.load(Ordering::SeqCst),
            0,
            "논의 중에는 부르지 않는다"
        );
        task.abort();
        let _cancelled = task.await;
        assert_eq!(
            released.load(Ordering::SeqCst),
            1,
            "취소로 버려져도 한 번 부른다"
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    // 19차 1번의 시험 도움: 가짜 사무실 포트.
    struct NullSink;

    #[async_trait::async_trait]
    impl pi_office::EventSink for NullSink {
        async fn publish(
            &self,
            _kind: &str,
            _payload: serde_json::Value,
        ) -> Result<u64, pi_office::SinkError> {
            Ok(1)
        }
    }

    struct FixedClock;

    impl pi_office::Clock for FixedClock {
        fn now(&self) -> UnixMillis {
            UnixMillis(0)
        }
    }

    const TABLE_AGENTS: [pi_office::Agent; 4] = [
        pi_office::Agent::News,
        pi_office::Agent::Strategy,
        pi_office::Agent::Risk,
        pi_office::Agent::Leader,
    ];

    fn places(office: &pi_office::Office) -> Vec<pi_office::Place> {
        office
            .agents()
            .into_iter()
            .filter(|view| TABLE_AGENTS.contains(&view.agent))
            .map(|view| view.place)
            .collect()
    }

    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // 검증: 19차 1번 — 취소된 논의의 늦은 정리가 다음 논의 중이면 자리를 내리지 않는다(띄운 작업이 실제로 돌 때 본다).
    #[tokio::test]
    async fn 취소된_논의의_늦은_자리_내리기는_다음_논의가_열려_있으면_자리를_내리지_않는다() {
        let office = Arc::new(pi_office::Office::new(
            pi_office::TeamState::Operating,
            Arc::new(NullSink),
            Arc::new(FixedClock),
        ));
        let count = count();
        let old = DiscussingGuard::enter_with(
            &count,
            super::super::link::release_table(
                Some(Arc::clone(&office)),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
                Arc::clone(&count),
            ),
        );
        // 앞 논의가 버려지기 전에 다음 논의가 열려 탁자에 모인다.
        let next = DiscussingGuard::enter(&count);
        for agent in TABLE_AGENTS {
            office
                .set_activity(agent, pi_office::Activity::Discussing)
                .await
                .expect("다음 논의가 탁자에 모인다");
        }
        drop(old);
        // 띄운 정리 작업이 이제 돈다.
        settle().await;
        assert_eq!(
            places(&office),
            vec![pi_office::Place::Meeting; 4],
            "다음 논의가 열려 있으면 앞 논의의 늦은 정리가 자리를 내리지 않는다"
        );
        drop(next);
        // 같은 정리라도 열린 논의가 없으면 내린다.
        super::super::link::release_table(
            Some(Arc::clone(&office)),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            Arc::clone(&count),
        )();
        settle().await;
        assert_eq!(
            places(&office),
            vec![pi_office::Place::Desk; 4],
            "열린 논의가 없으면 자리로 돌아간다"
        );
    }
}
