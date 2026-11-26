//! (기능 G, R7-4 · N20 · E54) 결정 메모 정산의 거래일 달력.
//!
//! 정산은 결정 날에서 5거래일 뒤 날을 센다(`pi_journal::Journal::settle_memos`). 실제 시세 모드의 달력(시세 흐름이
//! 증권사에서 받은 것)은 오늘과 그 앞뒤 거래일 하루씩만 알아 지난 날들을 이어 셀 수 없다. 그래서 서버가 남긴
//! **하루 시작 · 마감 기록**([`crate::KIND_DAY_START`] · [`crate::KIND_DAY_CLOSE`] — 그 날 정규장 세션이 있을 때만
//! 남는다)을 지난 거래일 목록으로 쓴다. 새 표는 만들지 않는다.
//!
//! 어느 날의 다음 거래일은 이 차례로 정한다.
//!
//! 1. 그 날의 달력을 알면(조립이 준 달력 · 시연 달력, 또는 오늘) 그 달력의 다음 영업일.
//! 2. 아니면 기록된 거래일 가운데 그 날 뒤의 첫 날과, 오늘 달력의 앞 영업일이 그 날일 때의 오늘(휴장이면 다음
//!    영업일) 가운데 이른 것. 둘 다 없으면 모른다(메모는 정산 대기로 남는다).
//!
//! 서버가 꺼져 있어 기록이 빈 날은 거래일로 세지 않는다(보수적 — 늦게 정산되는 쪽).

use std::collections::BTreeSet;
use std::ops::Bound;

use chrono::NaiveDate;
use pi_journal::TradingCalendar;

use crate::{KIND_DAY_CLOSE, KIND_DAY_START, Trading};

/// 이벤트를 한 번에 읽는 개수.
const READ_PAGE: usize = 1_000;

fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// 저장된 하루 시작 · 마감 이벤트의 날짜들(서버가 그 날 정규장 세션을 본 거래일).
pub(super) fn recorded_days(store: &pi_store::Store) -> BTreeSet<NaiveDate> {
    let mut days = BTreeSet::new();
    let mut after = 0;
    loop {
        let events = match store.read_after(after, READ_PAGE) {
            Ok(events) => events,
            Err(error) => {
                tracing::warn!(%error, "거래일 기록을 읽지 못했다");
                break;
            }
        };
        let Some(last) = events.last() else { break };
        after = last.seq;
        let marks = events
            .iter()
            .filter(|e| e.kind == KIND_DAY_START || e.kind == KIND_DAY_CLOSE);
        for event in marks {
            let date = event.payload["date"]
                .as_str()
                .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok());
            if let Some(date) = date {
                days.insert(date);
            }
        }
    }
    days
}

/// 정산에 쓰는 달력: 주문 길의 달력 + 기록된 거래일.
pub(super) struct MemoCalendar<'a> {
    trading: &'a Trading,
    today: NaiveDate,
    recorded: BTreeSet<NaiveDate>,
}

impl<'a> MemoCalendar<'a> {
    /// 저장소의 거래일 기록을 읽어 만든다.
    pub(super) fn read(store: &pi_store::Store, trading: &'a Trading, today: NaiveDate) -> Self {
        MemoCalendar {
            trading,
            today,
            recorded: recorded_days(store),
        }
    }

    /// 오늘 달력의 앞 영업일이 `day`면 그 다음 거래일(오늘, 오늘이 휴장이면 다음 영업일).
    fn after_previous(&self, day: NaiveDate) -> Option<NaiveDate> {
        let calendar = self.trading.calendar_on(self.today)?;
        if calendar.previous_business_day.date != day {
            return None;
        }
        Some(if calendar.today.is_holiday() {
            calendar.next_business_day.date
        } else {
            calendar.today.date
        })
    }
}

impl TradingCalendar for MemoCalendar<'_> {
    fn next_trading_day(&self, _symbol: &str, date: &str) -> Option<String> {
        let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        if let Some(calendar) = self.trading.calendar_on(day) {
            return Some(ymd(calendar.next_business_day.date));
        }
        let recorded = self
            .recorded
            .range((Bound::Excluded(day), Bound::Unbounded))
            .next()
            .copied();
        [recorded, self.after_previous(day)]
            .into_iter()
            .flatten()
            .min()
            .map(ymd)
    }
}
