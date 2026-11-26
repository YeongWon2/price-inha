//! (기능 L · 결정 30) 팀 지침 잇기: 논의를 열 때 판단 글에 넣을 지침 줄 · 논의가 끝난 뒤 쓰임 · 하루 정리 때 승격 ·
//! 성적 세기 · 회고, 바뀔 때마다 [`super::KIND_GUIDELINE`] 이벤트.
//!
//! 원본 동작 근거: `docs/research/hermes-agent.md` 3.6 · `docs/research/openclaw.md` 3.6 (pi_journal `guideline` 모듈 문서).
//!
//! - 지침의 승격 · 고침 · 내림은 승인 없이 돈다(2층). 주문은 지금처럼 1층 검사를 그대로 거친다(여기서 바꾸지 않는다).
//! - AI를 부르지 않는다: 글은 기존 복기 · 교훈 · 계기 글과 고정 틀에서 만든다(결정 15 · N14).
//! - 이벤트에는 지침 번호 · 종류 · 까닭만 담는다(지침 글 · 비밀 없음).

use pi_agent::{Decision, DiscussionOutcome};
use pi_journal::{
    ClosedTrade, GuidelineBook, GuidelineChange, GuidelineRules, Promotion, guideline_candidates,
};
use pi_store::{HISTORY_PAGE_MAX, TradeId};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::{Crew, KIND_GUIDELINE};

/// 지침 문턱(조정자 해석 · 결정 30, 이름 붙은 상수는 `pi_journal::GUIDELINE_*`).
pub(super) const RULES: GuidelineRules = GuidelineRules::DEFAULT;

/// 1층 길로 보낸 후보 이벤트의 까닭.
const WHY_LAYER_ONE: &str = "layer_one";

/// 바뀐 것 하나의 이벤트 내용.
pub(super) fn change_payload(book: &GuidelineBook, change: &GuidelineChange) -> Value {
    let kind = book
        .guidelines
        .iter()
        .find(|g| g.id == change.id)
        .map(|g| g.kind.word());
    json!({
        "action": change.action.word(),
        "id": change.id,
        "kind": kind,
        "why": change.why,
        "into": change.into,
    })
}

/// 승격 · 회고의 이벤트 내용들(바뀐 차례, 1층 길로 보낸 후보는 끝에).
fn payloads(book: &GuidelineBook, promotion: &Promotion, retro: &[GuidelineChange]) -> Vec<Value> {
    let mut out: Vec<Value> = promotion
        .changes
        .iter()
        .chain(retro)
        .map(|change| change_payload(book, change))
        .collect();
    out.extend(promotion.to_rules.iter().map(|candidate| {
        json!({
            "action": "to_rules",
            "id": Value::Null,
            "kind": candidate.kind.word(),
            "why": WHY_LAYER_ONE,
            "into": Value::Null,
        })
    }));
    out
}

impl Crew {
    /// 그 매매(종목)의 논의에 넣을 지침 줄. 넣은 지침은 그 매매의 쓰임으로 장부에 남긴다.
    pub(super) fn guideline_lines(&self, trade: TradeId) -> Vec<String> {
        let symbol = match self.store.get_trade(trade) {
            Ok(found) => found.symbol,
            Err(error) => {
                tracing::warn!(%error, "논의할 매매를 읽지 못해 팀 지침 없이 엽니다");
                return Vec::new();
            }
        };
        self.guide.update(|book| {
            let picked: Vec<(String, String)> = book
                .select(&symbol, &RULES)
                .iter()
                .map(|g| (g.id.clone(), g.line()))
                .collect();
            let ids: Vec<String> = picked.iter().map(|(id, _)| id.clone()).collect();
            book.note_injected(trade.0, &ids);
            picked.into_iter().map(|(_, line)| line).collect()
        })
    }

    /// 논의가 끝났다: 사기면 사자 판단 수와 쓰임을 남기고, 지켜보기면 넣었던 피하기 지침의 막음을 센다. 팔기는
    /// 사지도 막지도 않았으니 그 매매의 쓰임만 지운다.
    pub(super) fn note_guideline_outcome(&self, trade: TradeId, outcome: &DiscussionOutcome) {
        self.guide.update(|book| match outcome.decision {
            Decision::Buy => book.note_decision(trade.0, true),
            Decision::Watch(_) => book.note_decision(trade.0, false),
            Decision::Sell => book.forget(trade.0),
        });
    }

    /// 하루 정리 때: 끝난 매매의 쓰임을 세고, 되풀이된 것을 올리고, 그날 처음이면 회고한다. 바뀐 것마다 이벤트.
    pub(super) async fn learn_guidelines(&self) {
        let now = self.clock.now();
        let at = now.timestamp_millis();
        let date = self.trading.date_of(now).format("%Y-%m-%d").to_string();
        let reviews = self.journal.reviews().unwrap_or_else(|error| {
            tracing::warn!(%error, "복기 기록을 읽지 못해 실수로는 지침을 올리지 않습니다");
            Vec::new()
        });
        let memos = self
            .store
            .decision_memos_page(None, HISTORY_PAGE_MAX)
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "결정 메모를 읽지 못해 교훈으로는 지침을 올리지 않습니다");
                Vec::new()
            });
        let closed = self.closed_trades().await;
        let candidates = guideline_candidates(&reviews, &memos, &closed, &RULES);
        let results: Vec<(i64, Decimal)> = closed.iter().map(|c| (c.trade, c.pnl)).collect();
        let events = self.guide.update(|book| {
            book.settle(&results, at);
            let promotion = book.promote(candidates, at, &RULES);
            let retro = if book.close_day(&date) {
                book.retrospect(at, &RULES)
            } else {
                Vec::new()
            };
            payloads(book, &promotion, &retro)
        });
        for payload in events {
            self.record_guideline(payload).await;
        }
    }

    /// 지침 이벤트 하나를 남긴다(실패하면 경고만).
    pub(super) async fn record_guideline(&self, payload: Value) {
        if let Err(error) = self.trading.record(KIND_GUIDELINE, payload).await {
            tracing::warn!(%error, "팀 지침 이벤트를 남기지 못했습니다");
        }
    }

    /// 손익을 아는 끝난 매매(팔림 · 복기됨)와 그 계기.
    async fn closed_trades(&self) -> Vec<ClosedTrade> {
        let records = match self.trading.sold_records().await {
            Ok(records) => records,
            Err(error) => {
                tracing::warn!(%error, "끝난 매매를 읽지 못해 지침 성적을 세지 않습니다");
                return Vec::new();
            }
        };
        records
            .into_iter()
            .filter_map(|record| {
                let pnl = record.pnl?;
                let trade = record.record_ref.parse::<i64>().ok()?;
                let trigger = self
                    .store
                    .get_trade(TradeId(trade))
                    .ok()
                    .and_then(|found| found.rationale.trigger);
                Some(ClosedTrade {
                    trade,
                    symbol: record.symbol,
                    trigger,
                    pnl,
                })
            })
            .collect()
    }
}
