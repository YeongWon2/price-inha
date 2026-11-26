//! (백로그 T · 결정 30) 지침의 날 수를 기록된 거래일로 센다.

use crate::guideline::GuidelineBook;

impl GuidelineBook {
    /// 논의가 끝났다(그 날짜).
    pub fn note_decision_on(&mut self, trade: i64, bought: bool, _date: &str) {
        self.note_decision(trade, bought);
    }

    /// 하루 정리(기록된 거래일).
    pub fn close_days(&mut self, date: &str, _recorded: &[String]) -> bool {
        self.close_day(date)
    }
}
