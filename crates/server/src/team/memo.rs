//! (23번, R7-4 · M11 · C25 · E54 · N20) 결정 메모(어제 메모)와 팀을 잇는 곳.
//!
//! - 팀장 판단이 사기 · 팔기로 끝나 매매가 판단됨이 되면 결정 메모를 정산 대기로 남긴다
//!   (`pi_journal::Journal::record_decision`, 결정 날 = 그 시각의 시장 날짜). 지켜보기는 매매가 판단됨이 되지 않아
//!   메모가 없다(M11 "판단됨이 된 매매마다 하나").
//! - 하루 마감(복기 앞)에 정산할 것이 있으면 정산한다(`Journal::settle_memos`). 종가는 서버가 그날 정규장 마지막 시세를
//!   남기는 저장 이벤트(`reference_prices`)에서 읽고(그 날짜로 처음 남은 것), 기준 지수 시세는 없어 초과수익은 비운다.
//!   거래일은 주문 길의 달력이다. 교훈은 복기 담당 턴(원본: 2~4문장 반성, `docs/research/invest-agent-teams.md` D-1)이고
//!   하루 AI 호출 상한에 들되 팀장 예비분은 쓰지 않는다(공지 확인 답과 같은 몫). 몫이 없거나 답이 없으면 그 메모는
//!   정산 대기로 남아 다음 마감에 다시 한다(journal 규칙). 닫힌 메모마다 `office.memo` `{"trade", "state"}`를 남긴다.
//! - 다음 논의의 자료에 지난 교훈을 넣는다(`Journal::memo_brief`: 같은 종목 최근 5건의 결정 + 교훈, 다른 종목 최근
//!   3건의 교훈만). agent에 팀장만 받는 길이 없어 논의 자료(브리핑) 한 줄로 넣는다.
//!
//! 엔진(`settle_memos`)은 포트를 동기로 부른다. 그래서 AI는 두 번에 나눠 쓴다: 첫 셈에서 교훈이 필요한 메모를 모으고
//! (이때 정산 못 함은 닫힌다), 그 메모들의 교훈을 비동기로 받은 뒤, 받은 교훈으로 한 번 더 셈한다.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::NaiveDate;
use pi_agent::{DataItem, Decision, DiscussionOutcome, Role};
use pi_journal::{ClosePrices, LessonInput, LessonWriter, Redactor, SettlePorts, TradingCalendar};
use pi_store::{DecisionMemo, DecisionMemoState, TradeId, TradeSide, UnixMillis};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::Crew;
use super::demo_ai::REVIEW_HEAD;
use crate::reach::KIND_OFFICE_MEMO;
use crate::{SecretsRedactor, Trading};

/// 서버가 그날 정규장 마지막 시세를 남기는 저장 이벤트 종류(`trading::watch`의 것과 같은 이름).
const KIND_REFERENCE_PRICES: &str = "reference_prices";
/// 이벤트를 한 번에 읽는 개수.
const READ_PAGE: usize = 1_000;
/// 한 번의 마감에 교훈을 묻는 가장 많은 메모 수(남은 것은 다음 마감에 한다).
const LESSONS_PER_CLOSE: usize = 20;
/// 논의 자료에 넣는 어제 메모의 이름.
pub(super) const MEMO_SOURCE_NAME: &str = "어제 메모";
/// 교훈 턴의 머리(복기 담당 턴이라 [`REVIEW_HEAD`] 뒤에 붙는다).
pub(super) const LESSON_HEAD: &str = "[어제 메모]";

fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// 만분율 → `+1.23%` 글자(0은 부호 없음).
pub(super) fn pct(bp: i64) -> String {
    let sign = match bp.signum() {
        1 => "+",
        -1 => "-",
        _ => "",
    };
    let abs = bp.unsigned_abs();
    format!("{sign}{}.{:02}%", abs / 100, abs % 100)
}

fn side_word(side: TradeSide) -> &'static str {
    match side {
        TradeSide::Buy => "사기",
        TradeSide::Sell => "팔기",
    }
}

/// 그날 종가들: 날짜 → 종목 → 가격(그 날짜로 처음 남은 것).
struct Closes(HashMap<String, HashMap<String, Decimal>>);

impl Closes {
    fn read(store: &pi_store::Store) -> Closes {
        let mut by_day: HashMap<String, HashMap<String, Decimal>> = HashMap::new();
        let mut after = 0;
        loop {
            let events = match store.read_after(after, READ_PAGE) {
                Ok(events) => events,
                Err(error) => {
                    tracing::warn!(%error, "종가 기록을 읽지 못했다");
                    break;
                }
            };
            let Some(last) = events.last() else { break };
            after = last.seq;
            for event in events.iter().filter(|e| e.kind == KIND_REFERENCE_PRICES) {
                absorb(&mut by_day, &event.payload);
            }
        }
        Closes(by_day)
    }
}

fn absorb(by_day: &mut HashMap<String, HashMap<String, Decimal>>, payload: &Value) {
    let (Some(date), Some(prices)) = (payload["date"].as_str(), payload["prices"].as_object())
    else {
        return;
    };
    let day = by_day.entry(date.to_string()).or_default();
    for (symbol, price) in prices {
        let parsed = price.as_str().and_then(|text| text.parse::<Decimal>().ok());
        if let Some(price) = parsed {
            day.entry(symbol.clone()).or_insert(price);
        }
    }
}

impl ClosePrices for Closes {
    fn close(&self, symbol: &str, date: &str) -> Option<Decimal> {
        self.0.get(date)?.get(symbol).copied()
    }

    fn benchmark_close(&self, _symbol: &str, _date: &str) -> Option<Decimal> {
        None
    }
}

/// 주문 길의 달력. 날짜의 달력을 모르면(시세 흐름 달력은 오늘 것만 안다) 오늘 달력의 앞 영업일만 잇는다.
struct Calendar<'a> {
    trading: &'a Trading,
    today: NaiveDate,
}

impl TradingCalendar for Calendar<'_> {
    fn next_trading_day(&self, _symbol: &str, date: &str) -> Option<String> {
        let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        if let Some(calendar) = self.trading.calendar_on(day) {
            return Some(ymd(calendar.next_business_day.date));
        }
        let calendar = self.trading.calendar_on(self.today)?;
        if calendar.previous_business_day.date != day {
            return None;
        }
        let next = if calendar.today.is_holiday() {
            calendar.next_business_day.date
        } else {
            calendar.today.date
        };
        Some(ymd(next))
    }
}

/// 첫 셈: 교훈이 필요한 메모를 모으고 답하지 않는다.
#[derive(Default)]
struct Wanted(Mutex<Vec<LessonInput>>);

impl LessonWriter for Wanted {
    fn lesson(&self, input: &LessonInput) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(input.clone());
        None
    }
}

/// 둘째 셈: 받아 둔 교훈.
struct Answered(HashMap<TradeId, String>);

impl LessonWriter for Answered {
    fn lesson(&self, input: &LessonInput) -> Option<String> {
        self.0.get(&input.trade).cloned()
    }
}

/// 비밀 보관소의 가리기.
struct Hide<'a>(&'a SecretsRedactor);

impl Redactor for Hide<'_> {
    fn redact(&self, text: &str) -> String {
        pi_gate::Redactor::redact(self.0, text)
    }
}

/// 복기 담당의 답에서 교훈 글: JSON 객체면 `note`, 아니면 글 그대로. 빈 글이면 없음.
pub(super) fn lesson_of(text: &str) -> Option<String> {
    let lesson = match serde_json::from_str::<Value>(text.trim()) {
        Ok(Value::Object(object)) => object.get("note")?.as_str()?.trim().to_string(),
        Ok(_) => return None,
        Err(_) => text.trim().to_string(),
    };
    (!lesson.is_empty()).then_some(lesson)
}

/// 교훈 턴의 입력 글.
fn lesson_prompt(input: &LessonInput) -> String {
    let excess = input.excess_bp.map_or_else(|| "없음".to_string(), pct);
    format!(
        "{REVIEW_HEAD} {LESSON_HEAD} 매매 {trade} · 종목 {symbol} · 결정 날 {date} · 판단 {side}\n\
         가설: {hypothesis}\n계기: {trigger}\n\
         결과: 수익률 {ret} · 초과수익 {excess} · 보유 {days}거래일\n\
         정확히 2~4문장 평문으로 교훈을 쓴다. ① 이 기간 수익이 방향 판단에 대해 보여 주는 것(수치를 인용하고, \
         기간이 짧아 판단할 수 없으면 그렇게 쓴다) ② 논지 가운데 뒷받침되거나 반박된 부분 ③ 다음 비슷한 판단에 쓸 \
         교훈 하나. 답은 JSON 객체 하나다: {{\"mistakes\": [], \"note\": \"<교훈>\"}}",
        trade = input.trade.0,
        symbol = input.symbol,
        date = input.date,
        side = side_word(input.side),
        hypothesis = input.hypothesis.as_deref().unwrap_or("없음"),
        trigger = input.trigger.as_deref().unwrap_or("없음"),
        ret = pct(input.return_bp),
        days = input.holding_days,
    )
}

fn state_word(state: DecisionMemoState) -> &'static str {
    match state {
        DecisionMemoState::Pending => "pending",
        DecisionMemoState::Settled => "settled",
        DecisionMemoState::Unsettleable => "unsettled",
    }
}

impl Crew {
    /// 판단이 사기 · 팔기면 결정 메모를 정산 대기로 남긴다. 남기지 못하면 경고만 남긴다(논의는 그대로다).
    pub(super) fn note_decision(&self, trade: TradeId, outcome: &DiscussionOutcome) {
        let side = match outcome.decision {
            Decision::Buy => TradeSide::Buy,
            Decision::Sell => TradeSide::Sell,
            Decision::Watch(_) => return,
        };
        let now = self.clock.now();
        let date = ymd(self.trading.date_of(now));
        let at = UnixMillis(now.timestamp_millis());
        if let Err(error) = self.journal.record_decision(trade, &date, side, at) {
            tracing::warn!(%error, trade = trade.0, "결정 메모를 남기지 못했다");
        }
    }

    /// 하루 마감의 정산. 실패해도 마감(복기)은 계속한다.
    pub(super) async fn settle_day_memos(&self) {
        if let Err(error) = self.settle_memos_now().await {
            tracing::warn!(%error, "결정 메모를 정산하지 못했다. 다음 마감에 다시 한다");
        }
    }

    async fn settle_memos_now(&self) -> Result<(), pi_journal::Error> {
        let now = self.clock.now();
        let today = self.trading.date_of(now);
        let closes = Closes::read(&self.store);
        let calendar = Calendar {
            trading: &self.trading,
            today,
        };
        let hide = Hide(&self.redactor);
        let at = UnixMillis(now.timestamp_millis());
        let wanted = Wanted::default();
        let first = SettlePorts {
            prices: &closes,
            calendar: &calendar,
            lessons: &wanted,
            redactor: &hide,
        };
        let mut closed = self.journal.settle_memos(&ymd(today), &first, at)?;
        let inputs = std::mem::take(
            &mut *wanted
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        let answered = Answered(self.ask_lessons(inputs).await);
        if !answered.0.is_empty() {
            let second = SettlePorts {
                lessons: &answered,
                ..first
            };
            closed.extend(self.journal.settle_memos(&ymd(today), &second, at)?);
        }
        self.tell_closed(&closed, at);
        Ok(())
    }

    /// 교훈들을 복기 담당 턴으로 받는다. 하루 상한에서 팀장 예비분을 남기고 몫이 없으면 그만 묻는다.
    async fn ask_lessons(&self, inputs: Vec<LessonInput>) -> HashMap<TradeId, String> {
        let mut answers = HashMap::new();
        for input in inputs.into_iter().take(LESSONS_PER_CLOSE) {
            // (54차) 교훈 턴은 복기 담당의 호출 하나다(팀 · 담당별로 센다).
            if !self.take_spare_ai_call_by(super::link::office_agent(Role::Review)) {
                tracing::warn!("하루 AI 호출 상한이라 교훈을 다음 마감에 묻는다");
                break;
            }
            let title = format!("memo-{}", input.trade.0);
            let config = self.role(Role::Review);
            let asked = self
                .team
                .runner()
                .run_turn(
                    config,
                    pi_agent::TimeLimits::MARKET,
                    &title,
                    &lesson_prompt(&input),
                )
                .await;
            match asked.map(|outcome| lesson_of(&outcome.text)) {
                Ok(Some(lesson)) => {
                    answers.insert(input.trade, lesson);
                }
                Ok(None) => tracing::warn!(trade = input.trade.0, "교훈 답의 모양이 틀렸다"),
                Err(error) => tracing::warn!(%error, "복기 담당이 교훈을 답하지 못했다"),
            }
        }
        answers
    }

    /// 닫힌 메모마다 `office.memo`를 남긴다.
    fn tell_closed(&self, closed: &[DecisionMemo], at: UnixMillis) {
        for memo in closed {
            let payload = json!({"trade": memo.trade.0, "state": state_word(memo.state)});
            if let Err(error) = self.store.append_at(KIND_OFFICE_MEMO, &payload, at) {
                tracing::warn!(%error, "정산 알림을 남기지 못했다");
            }
        }
    }

    /// 다음 논의의 자료: 지난 결정 메모(N20). 정산된 것이 없으면 빈 목록.
    pub(super) fn memo_items(&self, symbol: &str) -> Vec<DataItem> {
        let now = UnixMillis(self.clock.now().timestamp_millis());
        let brief = match self.journal.memo_brief(symbol, now) {
            Ok(brief) => brief,
            Err(error) => {
                tracing::warn!(%error, "어제 메모를 읽지 못해 없이 논의한다");
                return Vec::new();
            }
        };
        if brief.same_symbol.is_empty() && brief.other_lessons.is_empty() {
            return Vec::new();
        }
        vec![DataItem {
            name: MEMO_SOURCE_NAME.to_string(),
            content: Some(brief_text(&brief)),
            known_at: now,
        }]
    }
}

/// 어제 메모 자료 글: 같은 종목은 결정 + 결과 + 교훈, 다른 종목은 교훈만.
fn brief_text(brief: &pi_journal::MemoBrief) -> String {
    let mut out = String::new();
    if !brief.same_symbol.is_empty() {
        out.push_str("같은 종목 지난 결정(최근 것부터):");
        for memo in &brief.same_symbol {
            let excess = memo.excess_bp.map_or_else(|| "없음".to_string(), pct);
            out.push_str(&format!(
                "\n- {} {} · 수익률 {} · 초과수익 {} · 보유 {}거래일 · 교훈: {}",
                memo.date,
                side_word(memo.side),
                memo.return_bp.map_or_else(|| "없음".to_string(), pct),
                excess,
                memo.holding_days.unwrap_or_default(),
                memo.lesson.as_deref().unwrap_or("없음"),
            ));
        }
    }
    if !brief.other_lessons.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("다른 종목 교훈:");
        for lesson in &brief.other_lessons {
            out.push_str(&format!("\n- {lesson}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{absorb, lesson_of, pct};
    use rust_decimal::Decimal;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn 만분율은_부호와_소수_둘째_자리까지_글로_바뀐다() {
        assert_eq!(pct(1000), "+10.00%");
        assert_eq!(pct(-505), "-5.05%");
        assert_eq!(pct(0), "0.00%");
        assert_eq!(pct(7), "+0.07%");
    }

    #[test]
    fn 교훈은_note_칸이나_글_그대로이고_빈_글은_없음이다() {
        assert_eq!(
            lesson_of(r#"{"note": " 교훈. "}"#).as_deref(),
            Some("교훈.")
        );
        assert_eq!(lesson_of("그냥 글.").as_deref(), Some("그냥 글."));
        assert_eq!(lesson_of(r#"{"mistakes": []}"#), None);
        assert_eq!(lesson_of(r#"{"note": "  "}"#), None);
        assert_eq!(lesson_of("[1]"), None);
        assert_eq!(lesson_of("   "), None);
    }

    #[test]
    fn 같은_날짜의_종가는_처음_남은_것을_쓴다() {
        let mut by_day = HashMap::new();
        absorb(
            &mut by_day,
            &json!({"date": "2026-11-21", "prices": {"000660": "100"}}),
        );
        absorb(
            &mut by_day,
            &json!({"date": "2026-11-21", "prices": {"000660": "200", "005930": "7"}}),
        );
        absorb(&mut by_day, &json!({"date": 3, "prices": {}}));
        let day = &by_day["2026-11-21"];
        assert_eq!(day["000660"], Decimal::from(100));
        assert_eq!(day["005930"], Decimal::from(7));
        assert_eq!(by_day.len(), 1);
    }
}
