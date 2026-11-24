//! (W5 1차 보강) 손실 기준: 통화별 그날 시작 평가액(하루 손실 한도, P3)과 최고 평가액(최대 낙폭, P4).
//!
//! 다시 켰을 때 다시 셈한 값과 달라질 수 있게 되면(그날 시작 뒤 평가액이 움직였거나, 보유 종목이 있는 채로
//! 하루를 시작했거나, 최고 평가액이 남긴 것보다 올랐으면) 저장소에 [`KIND_LOSS_BASIS`] 이벤트로 남기고,
//! 열 때 마지막 것을 되읽는다. 그래서 다시 켜도(시세가 아직 없어 보유 종목을 평균 단가로 셈해도) 기준이
//! 바뀌지 않는다. 아무 일도 없으면 남기지 않는다(띄우기만 해서는 이벤트가 늘지 않는다, V6).
//! 그날 시작은 남긴 날짜가 오늘일 때만 되읽는다(다른 날이면 그날 시작에 새로 잡는다).
//! 최고 평가액은 처음 돈 아래로 내려가지 않는다.

use std::collections::HashMap;

use chrono::NaiveDate;
use pi_market::Currency;
use pi_paper::InitialCash;
use pi_rules::Book;
use rust_decimal::Decimal;
use serde_json::{Map, Value, json};

/// 손실 기준 이벤트의 종류(저장만 하고 화면에 보내지 않는다).
pub(super) const KIND_LOSS_BASIS: &str = "loss_basis";

/// 통화별 그날 시작 · 최고 평가액과 마지막으로 셈한 장부.
#[derive(Default)]
pub(super) struct Marks {
    /// 그날 시작 평가액을 잡은 시장 날짜.
    day: Option<NaiveDate>,
    day_start: HashMap<Currency, Decimal>,
    peak: HashMap<Currency, Decimal>,
    /// 처음 돈(최고 평가액의 바닥).
    floor: HashMap<Currency, Decimal>,
    /// 저장소에 남긴(또는 다시 켜면 같게 셈해지는) 그날 시작의 날짜와 최고 평가액.
    saved_day: Option<NaiveDate>,
    saved_peak: HashMap<Currency, Decimal>,
    pub(super) last: Vec<Book>,
}

/// 저장할 때 쓰는 통화 글자. 장부를 셈하는 통화만 있다.
fn code(currency: &Currency) -> Option<&'static str> {
    match currency {
        Currency::Krw => Some("KRW"),
        Currency::Usd => Some("USD"),
        _ => None,
    }
}

/// 저장된 통화별 값 묶음을 읽는다(읽을 수 없는 값은 건너뛴다).
fn read_map(value: &Value, currencies: &[Currency]) -> HashMap<Currency, Decimal> {
    currencies
        .iter()
        .filter_map(|currency| {
            let text = value.get(code(currency)?)?.as_str()?;
            Some((currency.clone(), text.parse().ok()?))
        })
        .collect()
}

fn write_map(map: &HashMap<Currency, Decimal>) -> Value {
    let mut out = Map::new();
    for (currency, amount) in map {
        if let Some(code) = code(currency) {
            out.insert(code.to_string(), Value::String(amount.to_string()));
        }
    }
    Value::Object(out)
}

impl Marks {
    /// 저장된 마지막 기준(`saved`)에서 연다. 최고는 언제나, 그날 시작은 남긴 날짜가 `today`일 때만 되읽는다.
    /// 최고는 처음 돈 아래로 내려가지 않는다.
    pub(super) fn restore(
        saved: Option<&Value>,
        today: NaiveDate,
        initial: &InitialCash,
        currencies: &[Currency],
    ) -> Marks {
        let floor = HashMap::from([(Currency::Krw, initial.krw), (Currency::Usd, initial.usd)]);
        let mut marks = Marks {
            floor,
            ..Marks::default()
        };
        if let Some(saved) = saved {
            marks.peak = read_map(&saved["peak"], currencies);
            let date = saved["date"]
                .as_str()
                .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok());
            if date == Some(today) {
                marks.day = date;
                marks.day_start = read_map(&saved["day_start"], currencies);
            }
        }
        for (currency, floor) in &marks.floor {
            let peak = marks.peak.entry(currency.clone()).or_insert(*floor);
            *peak = (*peak).max(*floor);
        }
        marks.saved_day = marks.day;
        marks.saved_peak = marks.peak.clone();
        marks
    }

    /// 그 통화의 지금 평가액을 반영한다: 그날 시작이 없으면 지금 평가액(`today`로), 최고를 올린다.
    /// (그날 시작, 최고, 저장해야 하는가)를 돌려준다.
    pub(super) fn mark(
        &mut self,
        currency: &Currency,
        equity: Decimal,
        today: NaiveDate,
    ) -> (Decimal, Decimal, bool) {
        let day_start = *self.day_start.entry(currency.clone()).or_insert(equity);
        let day = *self.day.get_or_insert(today);
        let floor = self.floor.get(currency).copied().unwrap_or(equity);
        let peak = self
            .peak
            .entry(currency.clone())
            .or_insert_with(|| floor.max(equity));
        *peak = (*peak).max(equity);
        let peak = *peak;
        let day_moved = self.saved_day != Some(day) && equity != day_start;
        let peak_rose = self
            .saved_peak
            .get(currency)
            .is_none_or(|saved| peak > *saved);
        (day_start, peak, day_moved || peak_rose)
    }

    /// 그날 시작 평가액을 그 장부들의 평가액으로 새로 잡는다(`today`의 것으로). 바로 저장해야 하면 참이다
    /// (보유 종목이 있으면 다시 켰을 때 평균 단가로 셈해 달라지므로). 아니면 평가액이 움직일 때 남긴다.
    pub(super) fn reset_day(&mut self, books: &[Book], today: NaiveDate) -> bool {
        for book in books {
            self.day_start.insert(book.currency.clone(), book.equity);
        }
        self.day = Some(today);
        self.saved_day = None;
        for book in &mut self.last {
            book.day_start_equity = book.equity;
        }
        books.iter().any(|book| !book.positions.is_empty())
    }

    /// 저장할 내용: 날짜 · 통화별 그날 시작 · 최고. 아직 남긴 것으로 치지 않는다 — 저장이 성공한 뒤
    /// [`Marks::note_saved`]에 돌려준 표시를 넘겨야 남긴 것으로 친다(21차 2번: 저장이 실패하면 다음 셈이 다시 남긴다).
    pub(super) fn take_payload(&self) -> (Value, SavedMark) {
        let date = self.day.map(|d| d.format("%Y-%m-%d").to_string());
        let payload = json!({
            "date": date,
            "day_start": write_map(&self.day_start),
            "peak": write_map(&self.peak),
        });
        let saved = SavedMark {
            day: self.day,
            peak: self.peak.clone(),
        };
        (payload, saved)
    }

    /// 저장이 성공한 내용을 남긴 것으로 친다.
    pub(super) fn note_saved(&mut self, saved: SavedMark) {
        self.saved_day = saved.day;
        for (currency, peak) in saved.peak {
            let entry = self.saved_peak.entry(currency).or_insert(peak);
            *entry = (*entry).max(peak);
        }
    }
}

/// 저장한 기준(날짜 · 최고). 저장이 성공한 뒤에만 [`Marks::note_saved`]로 남긴 것으로 친다.
pub(super) struct SavedMark {
    day: Option<NaiveDate>,
    peak: HashMap<Currency, Decimal>,
}
