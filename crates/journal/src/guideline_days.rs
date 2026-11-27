//! (백로그 T · 결정 30) 지침의 날 수를 기록된 거래일로 센다.
//!
//! 30일 미사용 · 5일 막힘은 "하루 정리를 한 날"이 아니라 **거래일**로 센다. 서버가 켜져 있어 그 날 정규장 세션을 본
//! 거래일(server가 남긴 하루 시작 · 마감 기록)을 하루 정리 때 넘겨받아, 마지막 정리 뒤 · 오늘 앞의 기록된 날마다 날 수를
//! 하나씩 늘리고 그 날의 사자 판단 수를 하루 기록에 남긴다. 기록이 없는 날(서버가 꺼져 있던 날)은 세지 않는다
//! (결정 메모 정산의 달력과 같은 보수적 규칙). 처음 정리(마지막 정리가 없음)는 오늘만 센다.

use std::collections::BTreeSet;

use crate::guideline::{DayTally, GUIDELINE_TALLY_KEEP, GuidelineBook};

impl GuidelineBook {
    /// 논의가 끝났다(그 날짜 `YYYY-MM-DD`). [`GuidelineBook::note_decision`]과 같고, 샀으면 그 날짜의 사자 판단 수도
    /// 하나 늘린다(하루 정리가 그 날짜의 하루 기록에 넣는다).
    pub fn note_decision_on(&mut self, trade: i64, bought: bool, date: &str) {
        self.note_decision(trade, bought);
        if !bought {
            return;
        }
        match self.open_buys.iter_mut().find(|tally| tally.date == date) {
            Some(tally) => tally.buys = tally.buys.saturating_add(1),
            None => self.open_buys.push(DayTally {
                date: date.to_string(),
                buys: 1,
            }),
        }
    }

    /// 하루 정리(오늘 `date`, 기록된 거래일 `recorded`). 오늘이 마지막 정리 날이면 거짓이고 아무것도 바꾸지 않는다.
    ///
    /// 마지막 정리 뒤 · 오늘 앞의 기록된 날과 오늘마다(날짜 차례) 날 수를 하나 늘리고 하루 기록을 남긴다. 사자 판단은
    /// 판단한 날짜의 기록에 들고, 그 밖의 것(그 범위 밖 날짜 · 날짜 없이 센 것)은 오늘에 든다.
    pub fn close_days(&mut self, date: &str, recorded: &[String]) -> bool {
        if self.last_close.as_deref() == Some(date) {
            return false;
        }
        let days: BTreeSet<&str> = match self.last_close.as_deref() {
            Some(last) => recorded
                .iter()
                .map(String::as_str)
                .filter(|day| *day > last && *day < date)
                .collect(),
            None => BTreeSet::new(),
        };
        let open = std::mem::take(&mut self.open_buys);
        let dated: u32 = open.iter().map(|tally| tally.buys).sum();
        let mut today = self.today_buys.saturating_sub(dated);
        for tally in &open {
            if tally.date != date && !days.contains(tally.date.as_str()) {
                today = today.saturating_add(tally.buys);
            }
        }
        let buys_on = |day: &str| {
            open.iter()
                .filter(|tally| tally.date == day)
                .map(|tally| tally.buys)
                .sum::<u32>()
        };
        for day in days {
            self.push_day(day, buys_on(day));
        }
        let own = buys_on(date);
        self.push_day(date, own.saturating_add(today));
        let over = self.days.len().saturating_sub(GUIDELINE_TALLY_KEEP);
        self.days.drain(..over);
        self.today_buys = 0;
        self.last_close = Some(date.to_string());
        true
    }

    /// 날 수를 하나 늘리고 그 날의 하루 기록을 남긴다.
    fn push_day(&mut self, date: &str, buys: u32) {
        self.day = self.day.saturating_add(1);
        self.days.push(DayTally {
            date: date.to_string(),
            buys,
        });
    }
}
